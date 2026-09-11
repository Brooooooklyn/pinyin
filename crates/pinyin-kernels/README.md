# napi-pinyin-kernels

Safe interfaces for pinyin's text kernels. Default builds use portable Rust.
The optional `simd` feature enables native ARM64/x64 transcoding through
simdutf 0.7.0 and bounded NEON/SSE2 ASCII scanning, trie comparisons, and JSON scanning.
WASM builds additionally require the `simd128` target feature to enable their
vector kernels. All paths preserve standard Unicode replacement behavior.

The API accepts valid Rust strings or bounded slices. ASCII prefixes include
control bytes. UTF-16 conversion replaces lone surrogates. `find4` returns the
first matching lane; callers must exclude unused padding lanes.

Native transcoding builds C++ through simdutf. That dependency is excluded from
WASM and other native architectures. The pinyin core enables these routines
through an optional dependency and continues to forbid unsafe code itself.

Tests compare every Unicode scalar, malformed UTF-16, unaligned input, short
tails, and padded trie searches against scalar reference implementations.
