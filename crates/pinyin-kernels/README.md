# napi-pinyin-kernels

Safe interfaces for pinyin's text kernels. Default builds use portable Rust.
The optional `simd` feature enables Rust NEON transcoding on ARM64 and
runtime-detected SSSE3 on x64, plus NEON/SSE2 scanning and trie comparisons.
WASM builds additionally require the `simd128` target feature to enable their
vector kernels. All paths preserve standard Unicode replacement behavior.

The API accepts valid Rust strings or bounded slices. ASCII prefixes include
control bytes. UTF-16 conversion replaces lone surrogates. `find4` returns the
first matching lane; callers must exclude unused padding lanes.

`append_utf16`, `from_utf16_lossy`, and `decode` return `Result<T, Error>`.
Errors describe output-size overflow, failed reservations, or invalid decoder
positions. They implement `Display` and `std::error::Error`; reservation errors
retain their underlying source. An append may leave a valid prefix in the caller's
buffer on failure. Invalid UTF-16 is replaced, not reported as an error.
Scanners and fixed-width comparisons remain infallible.

There are no C++ or runtime dependencies. The pinyin core enables these routines
through an optional dependency and continues to forbid unsafe code itself.

Tests compare every Unicode scalar, malformed UTF-16, unaligned input, short
tails, and padded trie searches against scalar reference implementations.
