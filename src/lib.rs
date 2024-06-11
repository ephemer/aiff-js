#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use wasm_bindgen::prelude::*;

use core::arch::wasm32::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

fn read_float80(buffer: &[u8]) -> f64 {
    assert!(10 <= buffer.len(), "Buffer too small to contain a float80 at the given offset");

    let exponent = ((buffer[0] as u16 & 0x7F) << 8 | buffer[1] as u16) as i32 - 16383;

    let hi_mantissa = (buffer[2] as u32) << 24
        | (buffer[3] as u32) << 16
        | (buffer[4] as u32) << 8
        | (buffer[5] as u32);

    let lo_mantissa = (buffer[6] as u32) << 24
        | (buffer[7] as u32) << 16
        | (buffer[8] as u32) << 8
        | (buffer[9] as u32);

    let mantissa = (hi_mantissa as f64 + (lo_mantissa as f64 / (1u64 << 32) as f64)) / (1u64 << 31) as f64;
    let value = mantissa * 2f64.powi(exponent);
    if buffer[0] & 0x80 != 0 { -value } else { value }
}

const CHUNK_HEADER_BYTE_SIZE: usize = 8;

#[wasm_bindgen]
pub unsafe fn aiff_to_wav(aiff_data: &[u8]) -> Vec<u8> {
    let aiff_len = aiff_data.len();
    let mut offset = 12;

    let mut channels = 0;
    let mut bits_per_sample = 0;
    let mut sample_frames = 0;
    let mut sample_rate = 0.0;

    let mut ssnd_section_size = 0;
    let mut aiff_data_offset = 0;

    while offset < aiff_len {
        let chunk_id = u32::from_be_bytes([aiff_data[offset], aiff_data[offset + 1], aiff_data[offset + 2], aiff_data[offset + 3]]);
        let chunk_size = u32::from_be_bytes([aiff_data[offset + 4], aiff_data[offset + 5], aiff_data[offset + 6], aiff_data[offset + 7]]) as usize;

        if chunk_id == u32::from_be_bytes(*b"COMM") {
            channels = u16::from_be_bytes([aiff_data[offset + 8], aiff_data[offset + 9]]) as usize;
            sample_frames = u32::from_be_bytes([aiff_data[offset + 10], aiff_data[offset + 11], aiff_data[offset + 12], aiff_data[offset + 13]]) as usize;
            bits_per_sample = u16::from_be_bytes([aiff_data[offset + 14], aiff_data[offset + 15]]) as usize;
            sample_rate = read_float80(&aiff_data[offset + 16..offset + 26]);
        }

        if chunk_id == u32::from_be_bytes(*b"SSND") {
            ssnd_section_size = chunk_size - CHUNK_HEADER_BYTE_SIZE;
            aiff_data_offset = offset + CHUNK_HEADER_BYTE_SIZE + 4 + 4; // 4 bytes each for (normally unused) `offset`, `blockSize`
        }

        offset += CHUNK_HEADER_BYTE_SIZE + chunk_size;
    }

    let total_sample_byte_size = sample_frames * channels * bits_per_sample / 8;
    if ssnd_section_size != total_sample_byte_size {
        panic!("SSND section size incorrect. Maybe `offset` / `blockSize` were used in the AIFF file's SSND section.");
    } 

    let wav_data_size = total_sample_byte_size + 44; // 44 for wav header
    let mut wav_data = vec![0u8; wav_data_size];

    // Fill the WAV header
    wav_data[0..4].copy_from_slice(b"RIFF");
    wav_data[4..8].copy_from_slice(&(wav_data_size as u32 - 8).to_le_bytes());
    wav_data[8..12].copy_from_slice(b"WAVE");

    // fmt chunk
    wav_data[12..16].copy_from_slice(b"fmt ");
    wav_data[16..20].copy_from_slice(&16u32.to_le_bytes());
    wav_data[20..22].copy_from_slice(&1u16.to_le_bytes());
    wav_data[22..24].copy_from_slice(&(channels as u16).to_le_bytes());
    wav_data[24..28].copy_from_slice(&(sample_rate as u32).to_le_bytes());
    wav_data[28..32].copy_from_slice(&(sample_rate as u32 * channels as u32 * bits_per_sample as u32 / 8).to_le_bytes());
    wav_data[32..34].copy_from_slice(&(channels as u16 * bits_per_sample as u16 / 8).to_le_bytes());
    wav_data[34..36].copy_from_slice(&(bits_per_sample as u16).to_le_bytes());

    // data chunk
    wav_data[36..40].copy_from_slice(b"data");
    wav_data[40..44].copy_from_slice(&(total_sample_byte_size as u32).to_le_bytes());

    let data_offset = 44;

    // Note: we currently ignore any remainder that doesn't divide evenly by chunk_size
    // This 
    match bits_per_sample {
    8 => {
        let chunk_size: usize = 16;
        aiff_data[aiff_data_offset..aiff_data_offset + total_sample_byte_size].chunks_exact(chunk_size)
            .zip(wav_data[data_offset..data_offset + total_sample_byte_size].chunks_exact_mut(chunk_size))
            .for_each(|(aiff, wav)| {
                let input = v128_load(aiff.as_ptr() as *const v128);
                v128_store(wav.as_mut_ptr() as *mut v128, input);
            });
    }

    16 => {
        // The 16 items in a v128 divide evenly by 2, so we can just use one pattern to swizzle the byte order:
        let be_le_swizzle_16 = i8x16(
            1, 0, 3, 2,
            5, 4, 7, 6,
            9, 8, 11,10,
            13,12, 15,14
        );

        let chunk_size: usize = 16;
        aiff_data[aiff_data_offset..aiff_data_offset + total_sample_byte_size].chunks_exact(chunk_size)
            .zip(wav_data[data_offset..data_offset + total_sample_byte_size].chunks_exact_mut(chunk_size))
            .for_each(|(aiff, wav)| {
                let input = v128_load(aiff.as_ptr() as *const v128);
                let result = i8x16_swizzle(input, be_le_swizzle_16);
                v128_store(wav.as_mut_ptr() as *mut v128, result);
            });
    }

    24 => {
        // 16 does not divide evenly by 3, so we use a rotating pattern of 3
        // swizzle patterns (at which point the pattern repeats) to swap the
        // byte order from big endian to little endian.
        let be_le_swizzle_24_1 = i8x16(
            2, 1, 0,
            5, 4, 3,
            8, 7, 6,
            11,10,9,
            14,13,12,
            15,
        );

        let be_le_swizzle_24_2 = i8x16(
            1, 0, // this should be 0, 1, 15 (from the last chunk), so we need to swap 1 (idx 17) and 15
            4, 3, 2,
            7, 6, 5,
            10,9, 8,
            13,12,11,
            // we actually want 0, 15, 14 here, but we don't have access to the following `0` yet
            14,15 // so we read in this order to avoid having to do two swaps to keep things ordered
        );

        let be_le_swizzle_24_3 = i8x16(
            0,
            3, 2, 1,
            6, 5, 4,
            9, 8, 7,
            12,11,10,
            15,14,13
        );

        // The swizzle pattern repeats itself after 48 elements (16 * 3)
        let chunk_size: usize = 48;

        aiff_data[aiff_data_offset..aiff_data_offset + total_sample_byte_size].chunks_exact(chunk_size).into_iter()
            .zip(wav_data[data_offset..data_offset + total_sample_byte_size].chunks_exact_mut(chunk_size).into_iter())
            .for_each(|(aiff, wav)| {
                let input = v128_load(aiff.as_ptr() as *const v128);
                let result = i8x16_swizzle(input, be_le_swizzle_24_1);
                v128_store(wav.as_mut_ptr() as *mut v128, result);

                let input = v128_load(aiff.as_ptr().offset(16) as *const v128);
                let result = i8x16_swizzle(input, be_le_swizzle_24_2);
                v128_store(wav.as_mut_ptr().offset(16) as *mut v128, result);
                wav.swap(15, 17);

                let input = v128_load(aiff.as_ptr().offset(32) as *const v128);
                let result = i8x16_swizzle(input, be_le_swizzle_24_3);
                v128_store(wav.as_mut_ptr().offset(32) as *mut v128, result);
                wav.swap(30, 32);
            });
    }

    32 => {
        // 16 divides evenly by 4
        let be_le_swizzle_32 = i8x16(
            3, 2, 1, 0,
            7, 6, 5, 4,
            11,10,9, 8,
            15,14,13,12,
        );

        // The swizzle pattern repeats itself after 48 elements (16 * 3)
        let chunk_size: usize = 16;

        aiff_data[aiff_data_offset..aiff_data_offset + total_sample_byte_size].chunks_exact(chunk_size).into_iter()
            .zip(wav_data[data_offset..data_offset + total_sample_byte_size].chunks_exact_mut(chunk_size).into_iter())
            .for_each(|(aiff, wav)| {
                let input = v128_load(aiff.as_ptr() as *const v128);
                let result = i8x16_swizzle(input, be_le_swizzle_32);
                v128_store(wav.as_mut_ptr() as *mut v128, result);
            });
    }

    _ => panic!("Unsupported bit depth")
    }

    wav_data
}