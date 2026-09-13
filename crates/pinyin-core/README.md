# napi-pinyin-core

A standalone Rust Chinese-to-pinyin engine with no runtime or build dependencies beyond the standard library by default. An optional `jieba` feature adds word segmentation. The Node.js and WebAssembly bindings enable that feature while preserving legacy Node segmentation unless a contextual resolver is explicitly selected. The standalone core is a new API: its `phrases` flag directly enables phrase matching.

```toml
[dependencies]
pinyin-core = { package = "napi-pinyin-core", path = "path/to/pinyin/crates/pinyin-core" }
```

```rust
use pinyin_core::{convert, pinyin, readings, tokens, write_pinyin, Style};

fn main() -> Result<(), pinyin_core::Error> {
assert_eq!(convert("中国 / Rust", Style::Tone, false)?, ["zhōng", "guó", " / Rust"]);
assert_eq!(pinyin("重庆银行音乐", Style::Plain, true, " ")?, "chong qing yin hang yin yue");

// Character-mode iteration allocates nothing. Syllables are static strings;
// non-Han tokens identify unchanged byte ranges in the original input.
let input = "中文🙂";
for token in tokens(input, false)? {
    println!("{}", token.text(input, Style::Tone));
}

// Reuse output capacity across conversions. This appends to existing contents.
let mut output = String::with_capacity(1024);
write_pinyin("你好", Style::ToneNumberEnd, false, " ", &mut output)?;
assert_eq!(output, "ni3 hao3");
output.clear();

let all: Vec<_> = readings('中').map(|reading| reading.text(Style::Tone)).collect();
assert_eq!(all, ["zhōng", "zhòng"]);
Ok(())
}
```

`Style` supports plain, tone marks, numbers at the tone-bearing letter, numbers at the syllable end, and first letters. Neutral tones have no appended number. Adjacent unmapped Unicode scalars are preserved in a single token, including whitespace, emoji, supplementary characters, and NUL bytes.

`phrases: false` preserves pinyin 0.11.0 / pinyin-data 0.15.0 character readings and alternate-reading order. `phrases: true` applies 4,083 built-in phrase pronunciations using maximum-probability path selection. It changes contextual readings; it does not expose word boundaries. All-reading iteration always returns the character dictionary's alternatives.

The contextual mode does not implement every pinyin-pro feature: surname priorities, mutable custom dictionaries, automatic traditional-to-simplified phrase matching, numeric context rules, and productive tone-sandhi rules are outside its current scope. Some phrase entries themselves contain neutral or sandhi tones. Reproducing dictionary entries is not a claim of general linguistic accuracy.

## Errors

Conversion and token preparation return `pinyin_core::Result<T>`. Errors implement
`Display` and `std::error::Error`, preserving underlying encoding or allocation
failures as sources. The token iterator stays infallible after preparation;
lookup, reading iteration, and comparison remain infallible. Writers may have
appended a prefix when an error occurs; discard that call's output before retrying.
This change affects the new, unpublished Rust APIs. JavaScript signatures, return
values, and validation messages remain unchanged.

The Node workspace uses the vendored `Jieba::try_new` constructor and caches its
`Result`. Standalone consumers use upstream Jieba, whose initialization and HMM
error behavior are outside this crate's error contract. Standard collection growth
and third-party allocators are not guaranteed to recover from out-of-memory errors.

## Implementation

- Direct indexing for common CJK characters; deduplicated Unicode pages elsewhere.
- Precomputed style strings shared by all callers.
- Static phrase trie with direct root lookup and compact sorted child edges.
- Reverse dynamic programming with six rolling costs and two-byte choices.
- Borrowed non-Han ranges and a whole-ASCII fast path.
- Streaming comparison with precomputed syllable keys, early exit, and no allocated sort keys.
- No unsafe Rust, mutable global dictionaries, initialization locks, or thread pools in the core. CPU-specific kernels are an optional dependency.

Character iteration is O(n) time with O(1) auxiliary space. Phrase resolution is O(n L log d) worst case, with maximum phrase length L = 5 and child fanout d; it uses O(n) scratch space. Probability scores are negative logarithms, so long inputs do not underflow.

## Optional Jieba integration

```toml
[dependencies]
pinyin-core = { package = "napi-pinyin-core", path = "path/to/pinyin/crates/pinyin-core", features = ["jieba"] }
```

```rust
use pinyin_core::{jieba, Style};

fn main() -> Result<(), pinyin_core::Error> {
// Keep this instance for subsequent calls. Rust callers can customize its word
// dictionary using Jieba::add_word/load_dict before sharing it across threads.
let segmenter = jieba::Jieba::new();
let output = jieba::pinyin("重庆银行音乐", Style::Tone, &segmenter, false, " ")?;
assert_eq!(output, "chóng qìng yín háng yīn yuè");
Ok(())
}
```

The adapter also exposes `jieba::tokens` and `jieba::write_pinyin`. The boolean selects Jieba's HMM; Node uses `false`. Phrase selection adds one negative-log10 cost unit per crossed Jieba boundary, preferring word-aligned matches among competitive paths while preserving useful cross-word phrases. This avoids losing 分为 when Jieba cuts 划分/为. Unmatched characters retain their default reading. The adapter uses shared input-sized scratch buffers, with no per-word conversion allocation. Non-Han grouping and output separators remain consistent with the core API.

The caller owns Jieba initialization and dictionary updates; the core does not hold a global segmenter. Adding a segmentation word does not add a new pronunciation. Jieba boundaries can change which phrases are selected, so they do not guarantee higher pronunciation accuracy for every sentence. See the [integration report](../../docs/performance-research.md#jieba-integration) for current measurements and limits.

## Optional UTF-16 output

The dependency-free `utf16` feature adds precomputed UTF-16 syllables and direct output for engines that use two-byte strings. It leaves the default crate's tables and API available without that feature. No encoding cache, unsafe code, or C++ dependency is added to the core.

```rust
use pinyin_core::{utf16, Style};

fn main() -> Result<(), Box<dyn std::error::Error>> {
let units = utf16::pinyin("重庆银行", Style::Tone, true, " ")?;
assert_eq!(String::from_utf16(&units)?, "chóng qìng yín háng");
Ok(())
}
```

`utf16::write_pinyin` appends to a caller-owned `Vec<u16>` and accepts a UTF-16 separator. `Syllable::utf16(style)` exposes static syllable slices. With both features enabled, `jieba::pinyin_utf16` uses the same phrase decisions and HMM option as `jieba::pinyin`. Input and token ranges remain UTF-8. The Node binding separately provides SIMD transcoding at the JavaScript boundary; see the [implementation and benchmark report](../../docs/performance-research.md#utf-16-input-and-output).

## Optional SIMD kernels

The `simd` feature enables `napi-pinyin-kernels` for ASCII span scanning, packed trie comparisons, and selective UTF-8 decoding. Dense Chinese string output can reuse the decoded phrase scratch buffer; mixed and ASCII-heavy text keeps a byte cursor. With `utf16`, unchanged spans also use bulk transcoding. Default builds remain dependency-free.

Native transcoding uses Rust NEON on ARM64 and runtime-detected SSSE3 on x64, with portable fallbacks. The kernels have no C++ dependency. WebAssembly enables vector kernels only with `-C target-feature=+simd128`; ordinary WASM builds retain their existing host requirements.

The Node workspace additionally applies a pinned Jieba 0.10.3 patch for its private ARM64 character classifier. That Cargo patch does not propagate to standalone consumers of this crate, which can use the published Jieba dependency unchanged. See the [remaining SIMD optimization report](../../docs/performance-research.md#implemented-simd-and-output-paths) for benchmarks, the patch's scope, and validation.

## Validation

```sh
cargo test -p napi-pinyin-core --release
cargo test -p napi-pinyin-core --features jieba --release
cargo test -p napi-pinyin-core --features jieba,utf16 --release
cargo test -p napi-pinyin-core --features jieba,utf16,simd --release
```

The legacy pinyin dependency is used only by tests. Tests cover every valid Unicode scalar, every style and alternate reading, all phrase entries, overlapping phrases against an independent exhaustive solver, mixed Unicode, and comparator equivalence.

See the repository's [algorithm and performance report](../../docs/performance-research.md) for complete measurements, output-equivalence qualifications, and rejected optimization experiments.

## Dictionary provenance

Data is vendored and compiled during a normal Cargo build, without network access. [sources.json](data/sources.json) records upstream revisions and source hashes. The character data is the exact pinyin-data submodule used by pinyin 0.11.0; phrase entries come from pinyin-pro 3.29.3's built-in dictionaries. The implementation is maintained in this repository. The data retains its upstream MIT notices in `data/LICENSE.*`.

Dictionary updates must preserve the pinned source revisions, source hashes, and upstream license notices. Updates are explicit reviewable changes, never part of ordinary compilation.
