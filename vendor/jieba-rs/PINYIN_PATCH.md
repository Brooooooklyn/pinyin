# Pinned Jieba SIMD patch

Source: jieba-rs 0.10.3, commit c62e0df1f9dcc2cc1e014711c5aa4561ae260538.
All dictionary data and other library modules are unchanged from the published crate.
The MIT license is retained.

The patch also adds fallible `try_new` and `try_load_default_dict` methods using
upstream's existing error type. The Node binding uses only the fallible constructor
and `cut(..., false)`. Existing upstream constructors remain available for Rust
compatibility; their panic behavior, optional HMM/POS/keyword implementations,
and dependency-internal failures are outside the binding's fallible path.
The classifier uses normal iterator exhaustion instead of unwrapping characters.

The unused upstream WebAssembly test target and its wasm-bindgen-test development
dependency are omitted. This project builds WebAssembly through NAPI-RS for
wasm32-wasip1-threads and tests it through the generated NAPI-RS WASI loader.

The default cut classifier skips ASCII and common-CJK prefixes with bounded
NEON loads on ARM64 when the pinyin-simd feature is enabled. Other characters use the original Unicode predicate.
Other architectures retain the original loop. The sparse graph, probability
solver, HMM, word frequencies, token positions and public API are unchanged.

The root workspace applies this copy through Cargo patch configuration.
Standalone users of napi-pinyin-core can continue using published jieba-rs;
the root patch does not propagate to their workspaces. This patch is local
and has not been submitted upstream.
