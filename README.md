# aiff-js

Converts aiff files into wav in your browser on the fly, so you can play them back in Chrome.

Ignoring some exotic corners of the AIFF (and possibly also WAV) spec, AIFF files are essentially the same as WAV whose data are stored in big-endian byte order, as opposed to WAV's little-endian.

This repo implements a naive, non-streaming (i.e. the whole audio file is processed at once) implementation that converts AIFF to WAV on the fly, so you can play it back in Chrome. Of course, it would be much more efficient to implement this as a buffered decoder that converts audio chunk-by-chunk for memory and time efficiency. Since this is just a proof of concept I have left further efficiency improvements to the reader.


# Running

You can just open `js.html` and it will work without any further messing around. This is a pure JS implementation without any fancy tricks or optimizations.

## Rust + Wasm + SIMD

I ported the JS code to Rust to build it via Web Assembly. This already improves performance by about a 1.5-2x speedup. On top of that, I added SIMD support, whose main performance improvement comes by using SIMD swizzling to reverse the AIFF byte order.

Note that the SIMD implementation ignores the any frames at the end of the audio that don't divide evenly into `(128bit / your audio file's bitrate)` samples. In practice this will be no more than a few microseconds of audio, but technically it's not a fully correct conversion.

To use the Web Assembly + SIMD version, you will need to build it. Read on for how.


# Build

You will need [wasm-pack](https://github.com/rustwasm/wasm-pack) (`brew install wasm-pack`) and [rust](https://rustup.rs) installed.

Then build the rust version via:

```
wasm-pack build --target web
```

To load esmodules and wasm your code must be running on a web server. I use `npx http-server` but you can also use a python web server, or anything else that serves the local directory.

Assuming you ran `npx http-server`, simply visit http://127.0.0.1:8080 to try it out.


# Known Issues

Currently Float / Double input files are not supported, nor are esoteric bitrates like 12 bit (or 17 bit!) which are technically supported by the spec.


# Acknowledgements

I used [this](https://www-mmsp.ece.mcgill.ca/Documents/AudioFormats/AIFF/AIFF.html) document from McGill University, which links to a [PDF of the AIFF spec](https://www-mmsp.ece.mcgill.ca/Documents/AudioFormats/AIFF/Docs/AIFF-1.3.pdf) to get the details of the spec right to properly support 16, 24, and 32 bit integer files.