# SIMD research experiments

These historical experiments support the [SIMD candidate research](../../docs/performance-research.md#simd-candidate-research). They are outside the shipping Cargo workspace and npm package. The consolidated report distinguishes their results from the final implementation.

`src/main.rs` measures Unicode decoding, output transcoding, direct cached UTF-16 output, JSON escaping, and complete Rust output preparation. It asserts transcoding and escaping equivalence before timing. Its low-level simdutf wrappers only accept valid Rust strings, allocate bounded disjoint output storage, and publish initialized elements. The custom escape scanner uses NEON only on AArch64 and has a scalar fallback.

`build-probe.py` reconstructs the recorded pre-encoding sources in a temporary directory and builds a macOS addon with scalar/SIMD output selection. `node.ts` compares both modes in that same binary, checks outputs against the unchanged addon, and includes pinyin-pro comparisons, input-copy measurements, and unchanged ASCII controls. The experimental global selector is only for serial benchmarking. Source hashes are checked before injection, and the temporary directory is removed when the builder exits. Set the Node harness baseline to `target/encoding-baseline/before.node` after building it with `python3 benchmark/build-baseline.py encoding`.

```sh
CARGO_TARGET_DIR=target/simd-research cargo build --locked --release --manifest-path benchmark/simd-research/Cargo.toml
mkdir -p benchmark/results/simd-research
target/simd-research/release/pinyin-simd-research > benchmark/results/simd-research/kernels.jsonl
python3 benchmark/simd-research/build-probe.py
oxnode benchmark/simd-research/node.ts
python3 benchmark/simd-research/analyze.py
```

Run benchmarks sequentially after builds finish. The addon builder assumes macOS's dynamic-library filename; another host needs that name adapted. The report records limitations, portability requirements, and full result provenance. These probes are not production entry points.
