# Pinyin algorithms, SIMD optimization, and benchmarks

This is the consolidated research and implementation report for the standalone Rust core, optional Jieba integration, UTF-8/UTF-16 work, JSON escaping, and remaining SIMD paths. The implementation replaces per-character allocations and repeated JavaScript boundary work with static data and specialized output writers. It also makes contextual segmentation affect pronunciation. The old character lookup itself was already inexpensive.

The final Jieba-enabled binding is **1.93–7.75× faster than pinyin-pro 3.29.3** on the measured 100,000-character synthetic corpus with identical outputs, depending on output style and shape. Natural prose differs in dictionary coverage and reading policy; those timings are not an equivalent-accuracy comparison. The last optimization pass reduces long-ASCII string conversion time by **6.5–11.4×** relative to the preceding UTF-16 implementation. Smaller changes vary with desktop noise; neither result establishes an absolute performance limit.

- [Algorithms and compatibility](#algorithm-comparison)
- [Jieba integration](#jieba-integration)
- [UTF-8 validation](#utf-8-validation) and [UTF-16 boundaries](#utf-16-input-and-output)
- [Implemented SIMD paths](#implemented-simd-and-output-paths)
- [Final native measurements](#native-measurements) and [WebAssembly measurements](#webassembly-measurements)
- [Jieba patch and distribution cost](#jieba-patch-and-compatibility)
- [Rejected experiments](#experiments-that-did-not-become-the-implementation)
- [Validation and reproduction](#validation-and-reproduction)
- [Historical stage measurements](#historical-stage-measurements) and [sources](#sources)

## Scope and reference versions

The original baseline is a release build of commit `f4409a36802672f6677a8033d63109b16a8a2f40`. The final SIMD comparison instead uses the saved intermediate build that already includes Jieba, simdutf8, and UTF-16 output. Each stage below identifies its own baseline; results from separate stages must not be pooled or their speedups multiplied.

The measured JavaScript package is pinyin-pro **3.29.3**, with algorithm reference revision `14b898d6aff00c9df75be351667bcc4155da395b`. Character readings retain pinyin 0.11.0 / pinyin-data 0.15.0 compatibility. Builds use optimization level 3, LTO, and one code generation unit, without `target-cpu=native` or result memoization. Runtime and artifact hashes accompany the raw measurements.

All timings were collected on an active Apple M5 Max desktop with 128 GiB RAM, macOS ARM64, Rust 1.98.0, and Node 24.13.1. Complete-call comparisons match input encoding, styles, non-Han grouping, and return shape. Output equality is checked outside timing; returning strings from the original array-only binding includes `.join(' ')`. See the [evidence index](../benchmark/results/README.md) for each stage's samples and provenance.

## Algorithm comparison

| Layer            | Previous implementation                                                       | pinyin-pro                                                                     | New implementation                                                                             |
| ---------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Character lookup | Scan up to five Unicode blocks, then index static tables                      | Array indexed by UTF-16 code unit for BMP characters; a Map for longer strings | Direct common-CJK table; deduplicated 256-scalar pages elsewhere                               |
| Character data   | pinyin 0.11.0 / pinyin-data 0.15.0                                            | Its own curated character dictionary                                           | Same 41,923 mapped scalars and alternate-reading order as the old Rust data                    |
| Context          | Jieba cuts text; each resulting word is still iterated character by character | Aho–Corasick phrase matching and selectable path selection                     | Precompiled phrase trie and maximum-probability path selection                                 |
| Formatting       | Static style strings, copied to individual owned strings                      | Several middleware passes, including tone transformations                      | Static style strings borrowed by the iterator and copied directly into final output            |
| Non-Han text     | Collected as characters or rebuilt strings; several paths invoke Rayon        | Per-character result objects, optionally grouped afterward                     | Borrow original ranges; fast path for all-ASCII input                                          |
| JS arrays        | Rust string/vector allocation plus native calls for every element             | JavaScript arrays produced directly in the engine                              | Direct native construction below measured cutoffs; bulk JSON parsing above them                |
| Async work       | Build owned syllable strings on a worker, then create JS values               | Synchronous API                                                                | Worker computes readings and prepares bulk encoded output; main thread materializes JS results |
| Sort comparison  | Allocate full keys and perform 24 string replacements                         | No equivalent API used in this comparison                                      | Stream precomputed legacy sort-key bytes, stopping at the first difference                     |

### Existing Rust behavior

The old character lookup is a small, fixed-cost table operation. Its string implementation simply wraps `.chars()` and looks up each character. Consequently, calling it on a Jieba word does not select a phrase pronunciation. For example, the old segmented mode still uses the default readings for 重庆、银行、音乐. It incurs segmentation cost without obtaining the documented contextual benefit.[^old-binding][^old-core][^old-iterator]

The binding then converts each static syllable into an owned `String`. Non-Han text is buffered separately, sometimes sent to Rayon even when the useful operation is only concatenating a short run. The default output vectors reserve according to UTF-8 byte length rather than actual token count. These decisions add allocation, memory traffic, and scheduling beyond the dictionary lookup.[^old-binding]

### pinyin-pro behavior

pinyin-pro's character dictionary is not uniformly hash-based: `FastDictFactory` uses a numeric array for single-code-unit keys and a Map for supplementary characters. Its main conversion path also performs contextual processing, which makes a bare default-to-default timing comparison insufficient.[^pro-utils][^pro-entry]

Its phrase matcher constructs a trie with failure links. Matching walks the trie, visits failure states for suffix matches, and produces match objects. The selected segmentation algorithm processes these matches before the pronunciation pass. The current default is maximum probability, although the inspected speed benchmark explicitly selects reverse maximum matching.[^pro-ac][^pro-probability][^pro-speed]

The pronunciation pass expands matched phrases, falls back to character readings, and applies special-context rules. Subsequent middleware handles non-Han grouping, alternate readings, requested patterns, tones, and output shape. This provides a richer feature set, but entails multiple passes and intermediate objects.[^pro-handle][^pro-middleware]

### The standalone Rust core

The new character data deliberately retains legacy reading choices. The import verifies the character file byte-for-byte against the pinned pinyin-data submodule used by pinyin 0.11.0. All five output styles are computed at build time. Character lookup uses ordinary static strings and integer tables; small phrase-trie branches additionally use the SIMD side table described below.

Common CJK characters index a direct table. Other scalars use a page directory and a compact page payload. Identical pages, including empty pages, are shared. Lookup has a fixed number of loads, with no runtime search over Unicode blocks. Generated values are Rust integers, avoiding native-endian reinterpretation of serialized binary files.

The phrase dictionary contains 4,083 entries of lengths two through five. Root transitions use direct indexed pages. Other states hold contiguous, sorted child edges; two-to-four-child nodes use the padded SIMD label table when enabled, with scalar searches as the fallback and binary search for larger fanouts. Construction happens during the Cargo build using checked-in dictionary data, without fetching upstream dictionaries.

Phrase selection scans backward. For a position `i`, it compares an unmatched-character transition with every dictionary phrase starting there:

```text
cost[i] = min(13 + cost[i + 1], 7.698970004336019 + cost[i + phrase_length])
```

The constants are negative base-10 logarithms of the built-in probabilities, `1e-13` and `2e-8`. The algorithm does not multiply progressively smaller floating-point values. Only six costs must remain live because no phrase exceeds five characters. A two-byte choice per scalar records the selected path, then is rewritten in place into syllable overrides. An independent test compares this rolling trie solver with a full dictionary search and a full-sized dynamic-programming array.

Character iteration uses O(1) auxiliary storage. Contextual mode uses O(n) scratch storage: decoded scalars and two-byte choices. Its worst-case work is O(n L log d), where L is at most five and d is child fanout. With a fixed built-in dictionary, this is linear in input length. The bounded trie avoids both a runtime automaton build and a separate match-object list. This choice is appropriate for the shipped short phrases; an arbitrarily large mutable dictionary would merit another design.

## Native-boundary optimization

The iterator yields static syllable references and byte ranges into the original input. Small array results are written directly through Node-API, without intermediate owned syllable strings. The binding retains only a bounded prefix to select the bulk path, then streams tokens into the encoded result without collecting a full token vector. Asynchronous input buffers are copied before queuing work, so a caller cannot mutate or detach the data used by the worker. Bulk formatting and encoding also run on the worker; only JS array allocation remains on the environment thread.

For flat results of at least 32 tokens, or nested heteronym results of at least four tokens, the binding builds a JSON array and passes one string to a captured `JSON.parse` intrinsic. This moves bulk array construction into the JavaScript engine and avoids a native API call for each result element. Heteronym mode serializes nested arrays. It creates fresh arrays on every call, with no cached input results.

The parser reference is captured once for each Node-API environment, stored on that environment's owning thread, and removed by an environment cleanup hook. Later changes to `JSON.parse` or `String.prototype.split` do not affect it. No `eval`, generated JavaScript source execution, global instance-data slot, or cross-environment JS handles are used. The Node-API contract requires this environment isolation.[^node-api]

Boundary strings use a one-byte representation for ASCII and UTF-16 otherwise. The encoding experiment reduced conversion overhead for long tone-marked output. A tested WASI fallback routes ASCII strings containing NUL through UTF-16 because the installed runtime truncates its Latin-1 path at NUL.

All unconverted input text is JSON-escaped: quotes, backslashes, controls, and embedded NULs are covered. Astral characters and Unicode separators remain valid strings. UTF-8 byte inputs are validated before conversion. The standalone core forbids unsafe Rust.

Bulk serialization trades a temporary serialized buffer for fewer native calls. It does not eliminate the cost of allocating an array containing every requested token. Applications needing delimited text can call `pinyinString`, which writes one string directly and avoids this array construction entirely. Rust callers can use `write_pinyin` to reuse existing output capacity.

## Compatibility and pronunciation boundaries

The default remains plain pinyin, not tone-marked pinyin. Output stays `string[]`, or `string[][]` with `heteronym: true`. Adjacent non-Han characters remain grouped, empty input returns an empty array, and neutral tones receive no numeric suffix. UTF-8 errors and the asynchronous input snapshot guarantee remain intact.

`segment: true` now performs phrase pronunciation selection. This is an intentional behavior correction: 重庆银行音乐 becomes `chong qing yin hang yin yue`, while default character mode retains `zhong qing yin xing yin le`. With heteronyms enabled, both segment settings return the complete character alternatives in their legacy order. The flag selects pronunciation context; it does not return word boundaries.

This is not complete pinyin-pro API or pronunciation-policy compatibility. The core does not implement its surname modes, custom mutable dictionary priorities, numeric-context rules, productive tone-sandhi rules, or optional traditional-to-simplified phrase matching. Traditional characters still have character readings where the retained dataset supplies them. Individual phrase entries may already include neutral or sandhi tones.

Dictionary conformance is distinct from real-world pronunciation accuracy. Reading all 4,083 entries correctly proves that the imported dictionary and matcher work; it cannot establish an independent linguistic accuracy percentage. Natural-text output differences are recorded rather than hidden or counted as performance wins at equivalent accuracy.

## Jieba integration

The Node and WebAssembly bindings support `jieba-rs` 0.10.3 through an explicit `segmenter: 'jieba'` option. The standalone Rust crate provides the adapter behind an optional `jieba` Cargo feature. Default conversion and the existing phrase resolver remain available.

```ts
import { pinyin, pinyinString, asyncPinyin, PINYIN_STYLE } from '@napi-rs/pinyin'

const options = {
  segment: true,
  segmenter: 'jieba' as const,
  style: PINYIN_STYLE.WithTone,
}

pinyin('重庆银行音乐', options)
// ['chóng', 'qìng', 'yín', 'háng', 'yīn', 'yuè']
pinyinString('重庆银行音乐', options)
// 'chóng qìng yín háng yīn yuè'
await asyncPinyin('重庆银行音乐', options)
```

`segmenter` defaults to `'phrase'`. It selects the contextual resolver when `segment: true`; with `segment: false`, or with `heteronym: true`, the engine uses character readings and skips segmentation. Unknown segmenter names are rejected. Node uses `HMM=false`, matching the original integration's segmentation setting. Rust callers can supply their own Jieba instance, customize its word dictionary, and choose the HMM flag.

### What the integration does

1. A shared, lazily initialized Jieba instance cuts the input into words. Ordinary native calls and async workers reuse this immutable instance. Pure ASCII conversions bypass initialization.
2. The core resolves a globally consistent phrase path, adding one negative-log10 cost unit per crossed Jieba word boundary. This favors word-aligned candidates among competitive paths while retaining useful cross-boundary pronunciation phrases. The boundary weight is an explicit heuristic, not a learned accuracy guarantee. Unmatched characters retain their original default reading.
3. The adapter supplies one override buffer to the existing token iterator or string writer. It decodes the input once for resolution and records boundaries in a one-byte-per-scalar buffer, avoiding per-word conversion allocations. Jieba's own segmentation allocations remain. The ordinary phrase resolver compiles out boundary processing.
4. The binding preserves the existing bulk array path, direct string output, grouped non-Han ranges, UTF-8 validation, and async input snapshots. Async segmentation, initialization if needed, and bulk encoding run on the worker; JavaScript result allocation runs on the environment thread.

Jieba 0.10.3 exposes both Unicode scalar and byte offsets for its tokens. The adapter uses scalar offsets to slice its character and pronunciation buffers. Its shared use is supported by the upstream `Send` and `Sync` implementations. See [Jieba's API](https://docs.rs/jieba-rs/0.10.3/jieba_rs/struct.Jieba.html) and [token offset definitions](https://docs.rs/jieba-rs/0.10.3/jieba_rs/struct.Token.html).

This differs from the original binding at `f4409a36802672f6677a8033d63109b16a8a2f40`: that version segmented words, then still used individual character readings. The new adapter connects boundaries to the pronunciation resolver. A regression test uses overlapping pronunciation phrases 一号 and 号叫. Adding 号叫 to a caller-owned Jieba dictionary changes the selected reading in the constructed fragment 一号叫, and clearing the dictionary changes it back. Cross-boundary fallback retains 重庆 and the useful 分为/称为 readings. An independent full-dictionary solver checks the weighted trie against both HMM modes.

### Pronunciation limits

Jieba's word dictionary does not contain pinyin readings. The adapter uses the same 41,923-character and 4,083-phrase pronunciation data as the phrase-only core; adding a Jieba segmentation word does not add pronunciation data. Boundary preferences can change competing phrase choices, so integration alone does not establish a general accuracy improvement.

For example, the current built-in dictionaries produce `zhang da yi` for 长大衣 in all three tested engines, even when the intended meaning is a long overcoat (`chang da yi`). Segmentation does not resolve every contextual ambiguity. The retained example outputs expose this limitation; they are not an accuracy benchmark.

The core still does not implement all pinyin-pro surname, custom-pronunciation, numeric-context, traditional-normalization, and productive tone-sandhi policies. Natural-text timings therefore explicitly distinguish output format from pronunciation equivalence.

An initial strict-boundary implementation was rejected: Jieba split 划分/为 and 统称/为, suppressing 分为 and 称为 and incorrectly reverting 为 from wéi to wèi. This affected 40 output tokens in the repeated literary corpus. The final weighted resolver allows those dictionary phrases to cross the boundary. The rejected implementation's measurements are retained under `benchmark/results/jieba/strict-boundaries/` and are not mixed into final samples.

## UTF-8 validation

The shared `utf8` helper calls the safe `simdutf8::compat::from_utf8` API. It returns a borrowed string without copying. Synchronous array/string conversion and asynchronous buffer conversion all use it; async calls still copy their input before queueing work. The existing `InvalidArg` code and detailed error message are preserved.

The compatibility API reports the first invalid sequence, its byte offset, and its length, and stops before scanning the remaining input. The `basic` API checks the entire input and omits error details. Using `basic` plus a standard-library fallback preserves diagnostics but pays for a second scan on errors. [simdutf8 API documentation](https://docs.rs/simdutf8/0.1.5/simdutf8/)

The isolated measurements below compare all three approaches. The small valid-input advantage of `basic` did not justify its much slower rejection of malformed buffers; the production binding uses `compat`.

ARM64 selects NEON automatically. The default `std` feature enables runtime AVX2/SSE4.2 selection on x86; unsupported targets use the portable implementation. WebAssembly requires the compile-time `simd128` feature for SIMD. The standard Rust WASM build uses this validator's fallback; the separate `build:wasm:simd` enables it. This says nothing about SIMD instructions in other linked dependencies; see the full-module inspection below. No global CPU-specific flags were added. [simdutf8 implementation selection](https://docs.rs/simdutf8/0.1.5/simdutf8/#implementation-selection)

## UTF-16 input and output

1. **UTF-16 input extraction.** The pinyin APIs use N-API's UTF-16 string getter instead of asking the engine to calculate and write UTF-8. Native ARM64/x64 builds use a validating SIMD UTF-16-to-UTF-8 conversion for longer input. The temporary UTF-16 allocation is released before dictionary work. Core input and token ranges remain UTF-8: this is not a claim that the entire resolver runs on UTF-16. [Node-API string extraction](https://nodejs.org/api/n-api.html#napi_get_value_string_utf16).
2. **Direct tone-marked output.** The build script emits optional UTF-16 syllable tables. String output uses a specialized writer, including a separate loop for empty separators. Bulk flat and heteronym arrays stream UTF-16 JSON; short arrays create engine strings directly from static syllable slices. This removes both the intermediate UTF-8 tone output and its subsequent conversion. [Node-API UTF-16 string creation](https://nodejs.org/api/n-api.html#napi_create_string_utf16).
3. **SIMD conversion where encoding remains necessary.** Non-ASCII output in other styles and long unchanged text runs use `simdutf` 0.7.0, whose bundled C++ source is 7.7.1. Inputs shorter than 32 UTF-16 units and output runs shorter than 64 UTF-8 bytes avoid FFI. The scalar decoder reserves the UTF-8 upper bound once, avoiding repeated allocations on short Chinese input. Malformed UTF-16 reuses that allocation for standard replacement decoding. [Rust simdutf API](https://docs.rs/simdutf/0.7.0/simdutf/).

Plain, numeric, and initial array output uses UTF-8 JSON; tone output writes UTF-16 directly. Known syllables bypass JSON escape scanning. Unknown UTF-8 tokens now use `json-escape-simd`, while UTF-16 output retains the bounded escape finder and bulk transcoding. The core takes valid UTF-8, and lone JavaScript surrogates retain standard replacement decoding.

## Implemented SIMD and output paths

| Path                               | Change                                                                              | Scope                                                                                                                            |
| ---------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| ASCII inside mixed input           | Skip 32-byte blocks using NEON, SSE2, or WASM SIMD128, then handle the bounded tail | String output; controls remain unchanged text. The public token/array iterator retains its original loop after regression checks |
| Unchanged output spans             | Accumulate adjacent unmapped characters and append each complete span once          | UTF-8 strings copy directly; UTF-16 strings transcode the span in bulk                                                           |
| Phrase scratch decoding            | Use simdutf UTF-8-to-UTF-32 on sufficiently long, Chinese-heavy input               | Native ARM64/x64; scalar decoding elsewhere                                                                                      |
| Decoded input reuse                | Retain the phrase scratch characters for dense Chinese string output                | Both phrase and Jieba resolvers; arrays retain the faster streaming byte iterator                                                |
| Small trie branches                | Compare four padded labels at once; keep each trie node eight bytes                 | Nodes with two to four children; larger branches keep sorted scalar searches                                                     |
| Plain/numeric/initial array output | Use json-escape-simd 3.1.1 for unchanged text in the UTF-8 JSON writer              | Known dictionary syllables bypass escape scanning; tone output retains direct UTF-16                                             |
| WebAssembly                        | Add SIMD128 ASCII, trie, escape scanning, and UTF-8/UTF-16 transcoding kernels      | Explicit separate build; standard WASM remains available                                                                         |
| Jieba character classes            | Skip common CJK and allowed ASCII prefixes before the original Unicode predicate    | Pinned Jieba 0.10.3 patch, ARM64 and root `simd` feature only                                                                    |
| Comparison                         | Generate exact legacy-compatible syllable sort keys at build time                   | Removes repeated accent conversion; this is precomputation, not a SIMD claim                                                     |

The core still uses `#![forbid(unsafe_code)]`. Bounded intrinsics and native transcoding live behind safe interfaces in the new `napi-pinyin-kernels` crate. It has a portable default and an optional `simd` feature. The core itself remains dependency-free by default. The Node binding enables its additional SIMD paths by default; `--no-default-features` disables those optional paths. The separate simdutf8 validator still selects supported SIMD instructions independently of this feature.

The phrase decoder requires at least 512 UTF-8 bytes and at least 28 three-byte scalars among both the first and last 32 characters. Buffer reuse requires at least 128 characters and the same end samples. These are workload heuristics, not guarantees about the middle of an input. The full conversion remains correct for arbitrary valid Unicode. Mixed workloads can still choose the general scalar path, and pathological distributions can defeat a heuristic's performance prediction.

Small trie indices are packed into the existing node field with checked build-time bounds. Only nodes with two to four children receive a padded side-table entry, adding about 10 KB of labels. Matching a padding lane is explicitly rejected, including for NUL input. The probability recurrence, dictionary ordering, phrase scores, and Jieba boundary penalties are unchanged.

## Native measurements

<!-- native-results:start -->

Medians of complete synchronous calls with JavaScript string input. Times are microseconds; a negative time change means less time. All before/after outputs match. The full 112-row run also retains short inputs, ASCII-only input, heteronyms, sequential async calls, and all matched-corpus comparisons.

| Fixture / resolver / output                       | Before, µs | Current, µs | Time change |
| ------------------------------------------------- | ---------- | ----------- | ----------- |
| literature-100k / character / tone / pinyin       | 2160.20    | 2073.81     | -4.0%       |
| literature-100k / character / tone / pinyinString | 587.27     | 615.44      | +4.8%       |
| literature-100k / phrase / tone / pinyin          | 2550.31    | 2328.96     | -8.7%       |
| literature-100k / phrase / tone / pinyinString    | 999.75     | 798.18      | -20.2%      |
| literature-100k / jieba / tone / pinyin           | 4102.83    | 3930.69     | -4.2%       |
| literature-100k / jieba / tone / pinyinString     | 2409.24    | 2340.37     | -2.9%       |
| mixed-100k / character / plain / pinyin           | 1043.81    | 875.13      | -16.2%      |
| mixed-100k / character / tone / pinyinString      | 402.02     | 346.60      | -13.8%      |
| mixed-100k / phrase / plain / pinyin              | 1386.09    | 1065.67     | -23.1%      |
| mixed-100k / phrase / tone / pinyinString         | 810.75     | 540.86      | -33.3%      |
| mixed-100k / jieba / plain / pinyin               | 2031.39    | 1905.55     | -6.2%       |
| mixed-100k / jieba / tone / pinyinString          | 1459.55    | 1381.71     | -5.3%       |
| long-ascii / character / plain / pinyinString     | 209.52     | 18.37       | -91.2%      |
| long-ascii / character / tone / pinyinString      | 269.14     | 41.35       | -84.6%      |
| long-unicode / character / tone / pinyin          | 339.07     | 325.91      | -3.9%       |
| long-unicode / character / tone / pinyinString    | 371.63     | 290.17      | -21.9%      |
| long-escaped / character / plain / pinyin         | 782.33     | 634.94      | -18.8%      |
| long-escaped / character / tone / pinyinString    | 377.03     | 215.21      | -42.9%      |

Comparator calls use precomputed legacy-compatible keys:

| Comparison            | Before, µs | Current, µs | Time change |
| --------------------- | ---------- | ----------- | ----------- |
| short-chinese         | 0.104      | 0.091       | -12.3%      |
| common-chinese-prefix | 4.620      | 2.805       | -39.3%      |
| identical             | 6.675      | 4.245       | -36.4%      |
| accented              | 0.137      | 0.137       | -0.5%       |

The final Jieba-enabled binding was also compared with pinyin-pro **3.29.3**. The matched corpus is a deterministic 100,000-character synthetic corpus with identical outputs. Natural prose has dictionary/contextual reading differences and is not an equivalent linguistic workload. pinyin-pro uses `toneSandhi: false` and `nonZh: 'consecutive'`; the binding uses Jieba with HMM disabled. Ratios on different outputs describe runtime only, not accuracy.

| Fixture / output                       | Current, ms | pinyin-pro, ms | Runtime ratio | Outputs match |
| -------------------------------------- | ----------- | -------------- | ------------- | ------------- |
| literature-100k / plain / pinyin       | 4.089       | 31.781         | 7.77×         | no            |
| literature-100k / plain / pinyinString | 2.327       | 34.365         | 14.77×        | no            |
| literature-100k / tone / pinyin        | 3.931       | 13.700         | 3.49×         | no            |
| literature-100k / tone / pinyinString  | 2.340       | 15.320         | 6.55×         | no            |
| matched-100k / plain / pinyin          | 6.761       | 32.900         | 4.87×         | yes           |
| matched-100k / plain / pinyinString    | 4.532       | 35.112         | 7.75×         | yes           |
| matched-100k / tone / pinyin           | 6.618       | 12.746         | 1.93×         | yes           |
| matched-100k / tone / pinyinString     | 4.696       | 15.200         | 3.24×         | yes           |

Additional tone-output Buffer-input controls:

| Fixture / resolver / API                   | Before, µs | Current, µs | Time change |
| ------------------------------------------ | ---------- | ----------- | ----------- |
| literature-100k / character / pinyinString | 586.79     | 597.67      | +1.9%       |
| literature-100k / jieba / pinyinString     | 2449.53    | 2343.43     | -4.3%       |
| mixed-100k / character / pinyinString      | 377.53     | 313.03      | -17.1%      |
| mixed-100k / jieba / pinyinString          | 1440.60    | 1344.10     | -6.7%       |
| literature-100k / character / asyncPinyin  | 2123.11    | 2077.05     | -2.2%       |
| literature-100k / jieba / asyncPinyin      | 4237.72    | 4149.33     | -2.1%       |

<!-- native-results:end -->

The largest remaining increases in the full native run were the ASCII-only 300 KB array controls: 6.8% for plain and 7.4% for tone output. A separate nine-round recheck measured only 1.7% and 1.5%, respectively. Natural-text character-mode tone strings changed from a 4.8% increase in the full run to essentially unchanged in that recheck (650.61 versus 649.85 µs). Both runs are retained in [the control results](../benchmark/results/remaining-simd/final-controls.json). These small differences are not stable enough to attribute to SIMD; the larger gains should not be generalized to every input or API.

## WebAssembly measurements

<!-- wasm-results:start -->

Tone-output calls through the Node WASI loader, in milliseconds. The last column isolates the current SIMD build against the current standard build; it does not compare against the older baseline. All three variants produce identical outputs in these cases.

| Fixture / resolver / API                   | Before, ms | Current standard, ms | Current SIMD, ms | SIMD time change |
| ------------------------------------------ | ---------- | -------------------- | ---------------- | ---------------- |
| literature-100k / character / pinyin       | 4.875      | 4.575                | 4.476            | -2.2%            |
| literature-100k / character / pinyinString | 3.052      | 2.984                | 2.852            | -4.4%            |
| literature-100k / phrase / pinyin          | 4.974      | 5.021                | 4.818            | -4.0%            |
| literature-100k / phrase / pinyinString    | 3.447      | 3.408                | 3.274            | -3.9%            |
| literature-100k / jieba / pinyin           | 7.083      | 7.242                | 6.961            | -3.9%            |
| literature-100k / jieba / pinyinString     | 5.509      | 5.287                | 5.482            | +3.7%            |
| mixed / character / pinyin                 | 2.927      | 2.877                | 2.987            | +3.8%            |
| mixed / character / pinyinString           | 2.431      | 2.333                | 2.263            | -3.0%            |
| mixed / phrase / pinyin                    | 3.339      | 3.197                | 3.263            | +2.1%            |
| mixed / phrase / pinyinString              | 2.931      | 2.686                | 2.663            | -0.9%            |
| mixed / jieba / pinyin                     | 4.419      | 4.555                | 4.525            | -0.7%            |
| mixed / jieba / pinyinString               | 4.187      | 4.061                | 4.195            | +3.3%            |
| long-ascii / character / pinyin            | 2.329      | 2.246                | 1.917            | -14.6%           |
| long-ascii / character / pinyinString      | 2.384      | 2.038                | 1.801            | -11.7%           |
| long-ascii / phrase / pinyin               | 2.777      | 2.566                | 2.288            | -10.9%           |
| long-ascii / phrase / pinyinString         | 2.795      | 2.260                | 2.064            | -8.7%            |
| long-ascii / jieba / pinyin                | 3.676      | 3.648                | 3.460            | -5.2%            |
| long-ascii / jieba / pinyinString          | 3.932      | 3.505                | 3.227            | -7.9%            |

<!-- wasm-results:end -->

SIMD is most useful here on the long ASCII fixture, with 5–15% less time than the current standard build. Mixed-text results range from roughly 3% better to 4% worse, so this build remains an explicit option rather than a universal default. The earlier unguarded transcoder had larger mixed-text regressions; the final guarded version avoids those failed probes.

`yarn build:wasm:simd` writes the explicitly SIMD-dependent module and its loaders into `target/wasi-simd`. The normal build command continues to produce the standard WASM module. Both measured modules require an engine supporting SIMD128; there is no implicit deployment switch or runtime replacement of the normal module. Rust documents this target feature and host requirement in its [WASM intrinsic reference](https://doc.rust-lang.org/core/arch/wasm32/index.html).

Artifact inspection found **792 SIMD instructions in the saved baseline**, **796 in the current standard build**, and **1,885 in the explicit SIMD build**. Wasmtime with SIMD disabled rejects the saved baseline. The raw benchmark label `portable` means the normal Rust build without `+simd128` for our kernels; it does **not** mean the complete linked module works on engines without SIMD. The explicit flag enables the additional pinyin and simdutf8 paths. These instruction counts identify feature use, not the contribution of any particular dependency or a cycle-level profile.

The WASM transcoder handles complete ASCII blocks and three-byte BMP blocks in vectors, then falls back to standard scalar Unicode conversion. Failed block probes regressed mixed text, so it now samples both ends and selects vectors only for predominantly ASCII/BMP inputs. UTF-16 input must have at least 128 units; UTF-8 spans at least 512 bytes. Each 32-character/unit end sample must contain at least 31 ASCII values or 28 three-byte/non-surrogate BMP values. Supplementary characters and lone UTF-16 surrogates retain standard replacement behavior, including inside an otherwise vector-selected input. SIMD UTF-8 validation is also enabled in simdutf8 by the target flag. Measurements load copies containing only the release `.wasm`, loader, and worker: the generated loader otherwise prefers a neighboring `.debug.wasm`, which would obscure the measured artifact identity.

## Jieba patch and compatibility

Jieba's relevant classifier is private, so the Node workspace uses a local Cargo patch of the published 0.10.3 source at commit `c62e0df1f9dcc2cc1e014711c5aa4561ae260538`. The additional `pinyin-simd` feature is enabled only by the binding's `simd` feature. All three data files and all other source modules match the published crate byte for byte. The upstream [MIT license](https://github.com/messense/jieba-rs/blob/c62e0df1f9dcc2cc1e014711c5aa4561ae260538/LICENSE) is retained.

The fast classifier accepts only prefixes that the original default cut predicate accepts. ASCII checks cover letters, digits, and `+#&._%-`. The CJK kernel loads 48 bytes, validates the three-byte layout, reconstructs 16 code points, and accepts the leading U+4E00–U+9FFF range. Other Unicode ranges, gaps, punctuation, emoji, other classifiers, and other architectures retain the original predicate. Dictionary traversal, the sparse graph, route probabilities, HMM, token positions, and the public API are unchanged.

A separate executable links both the unpatched registry dependency and the patched crate. It compared **7,128 complete segmentations**, including word strings and character/byte offsets, with default and custom dictionaries and both HMM modes. Every result matched. The parity corpus includes random Unicode, classification boundaries, CJK gaps, control bytes, emoji, long ASCII, and natural Chinese prose. See [parity evidence](../benchmark/results/remaining-simd/jieba-parity.json), [source hashes](../benchmark/results/remaining-simd/jieba-vendor.json), and the [minimal upstream patch](../benchmark/results/remaining-simd/jieba-upstream.patch).

The earlier same-binary classifier experiment reduced complete HMM-disabled Jieba segmentation on the prose fixture from 1.683 to 1.509 ms, about 10.4%. Its mixed-text result was about 1.8% and overlaps desktop noise; its long ASCII fixture improved about 6%. These isolate the classifier experiment and are separate from the final combined binding measurements. Raw samples and build provenance are in [the results directory](../benchmark/results/remaining-simd).

Vendoring preserves the entire published crate, including optional keyword/POS data. It adds about **14.0 MB of source**; the npm dry-run archive is about **5.33 MB compressed / 15.18 MB unpacked** including all project sources. This source-distribution cost is separate from the much smaller native binary increase. The patch has not been submitted upstream. Cargo patches do not propagate to downstream workspaces: a standalone consumer of `napi-pinyin-core` uses published Jieba unless it explicitly supplies its own patch. A standalone executable using the core with `jieba,utf16,simd` and published Jieba was built and run successfully.

The ARM64 release addon grows from 4,481,808 to 4,531,392 bytes (**1.1%**). Standard WASM grows from 3,541,152 to 3,576,432 bytes (**1.0%**); the explicit SIMD build is 3,583,398 bytes. The native build with optional SIMD disabled is 4,331,280 bytes. Exact measured artifact identities, source hashes, package contents, WASM feature evidence, and validation results are recorded in [final-state.json](../benchmark/results/remaining-simd/final-state.json).

## Experiments that did not become the implementation

Unconditional SIMD decoding regressed mixed input in the earlier research, so it remains selective. Retaining decoded characters for every output path also slowed ASCII-heavy text. A generic cursor and closure-based shared writer regressed natural Chinese conversion; those implementations were replaced by direct, statically specialized UTF-8 and UTF-16 loops. Arrays continue to use `CharIndices`, while strings can reuse dense Chinese scratch data.

Putting an ASCII scanner into the public token iterator accelerated long ASCII arrays but regressed dense Chinese and long mixed-Unicode arrays. A longer recheck confirmed the losses. Moving scanning to the unmapped branch and replacing the cursor did not consistently resolve them, so the final implementation restores the original token iterator. ASCII string scanning remains enabled. The JSON escape helper is kept outside the common dictionary-only loop to avoid pulling the larger escape kernel into that loop's compiled code. These decisions preserve measured improvements without claiming every SIMD candidate was successful. The intermediate array experiments and pre-guard native/WASM results remain in the results directory with distinct names.

The temporary addon supports independent trie, decode, and reuse switches. Its initial run checked **97,280 output comparisons** and timed all eight switch combinations. On natural phrase string output, the combined mode took 0.851 ms versus 0.955 ms with all three disabled. This development experiment preceded the final selective reuse guard and Jieba patch; it is supporting evidence, not the final before/after claim. The original builder snapshot and artifact hashes are retained alongside `ablation.json` and `probe.json`.

No attempt was made to replace dependent trie traversal or the backward probability recurrence with a SIMD loop. The available independent work is in scanning, decoding, and tiny label groups; a vector instruction does not remove dependent dictionary loads or JavaScript result allocation. Large arrays still pay for V8 parsing and object creation.

## Validation and reproduction

The local validation covered:

- 210 binding tests in each of native SIMD, native portable, standard WASI, and SIMD WASI builds.
- All six kernel tests in native portable, ARM64 SIMD, x64 SIMD under Rosetta, and WASM SIMD under Wasmtime. These exercise every Unicode scalar, malformed UTF-16, unaligned slices, block tails, escape positions, ASCII runs, and padded trie searches.
- Nine dependency-free core tests; ten core correctness tests plus five Jieba tests with `jieba,utf16,simd`, executed on ARM64 and x64. These include all dictionary readings/styles, every phrase, an independent exhaustive phrase solver, byte ranges, reused output buffers, and legacy comparator equivalence.
- Three binding encoding tests with and without SIMD; 7,128 independent Jieba parity cases; benchmark output equality outside timed loops.
- Portable core and kernels checked for i686 Linux with `utf16,simd`. The broader check including Jieba could not compile its transitive zstd code because the host lacks `i686-linux-gnu-gcc`.
- Formatting, TypeScript checks for the new harnesses, lint, and strict workspace Clippy with default features and without them.
- Kernel crate packaging and verification, core package file listing, standalone core consumer execution, and npm package file inclusion. Publishing the core with its optional registry dependency requires publishing `napi-pinyin-kernels` first; no package was published.

At the time these local measurements were recorded, remote CI, execution on 32-bit/Linux/Windows hardware, browser execution, sanitizers, and native x64 performance measurements had not run. Consult the PR checks for subsequent CI status. x64 correctness under Rosetta does not establish x64 hardware speedups. CI now includes the kernel tests and an explicit SIMD WASI build/test step.

```sh
yarn build
yarn build --no-default-features --output-dir target/remaining-simd-scalar
WASI_SDK_PATH=/opt/wasi-sdk yarn build --target wasm32-wasip1-threads
WASI_SDK_PATH=/opt/wasi-sdk yarn build:wasm:simd

cargo test -p napi-pinyin-core --features jieba,utf16,simd --release
cargo test -p napi-pinyin-kernels --features simd --release
python3 benchmark/remaining-simd/verify-jieba.py

# Reconstruct the saved source baseline in an isolated temporary directory.
python3 benchmark/build-baseline.py remaining-simd
PINYIN_BASELINE=target/remaining-simd-baseline/before.node \
  BENCH_ROUNDS=7 BENCH_MS=75 BENCH_FILTER='(/string$|compare)' \
  oxnode benchmark/remaining-simd.ts
oxnode benchmark/remaining-simd/wasm.ts
```

The WASM harness expects release modules with their matching loader/worker in `target/remaining-simd-wasi/{before,portable,simd}`. Results and implementation identities are retained under [`benchmark/results/remaining-simd`](../benchmark/results/remaining-simd). The baseline native SHA-256 is `f9d231da88159d27ff2d30a79a8cedb5c003d3e0d2620f8ed65c3f113626daf4`.

The native harness rotates implementation order over seven rounds of at least 75 ms, after warmup; the WASM harness uses five rounds of at least 60 ms. Each row retains input/output hashes and raw timing samples. Native timings include the complete N-API call, output construction, and array parsing; asynchronous rows await calls sequentially. They measure warm operation rather than first-call dictionary initialization. All results were collected on an active Apple M5 Max desktop with 128 GiB RAM, Rust 1.98.0, and Node 24.13.1. Builds/tests did not overlap the timed runs, but the machine was not isolated or thermally controlled. Small percentage differences should be treated as inconclusive rather than universal wins or a performance ceiling.

## Historical stage measurements

The following appendices preserve the measurements that guided the implementation. “New”, “current”, “before”, and “after” inside these tables refer to that historical stage, not the final PR build. These runs use different baselines and sampling windows. The final implementation and validation above supersede prototype recommendations and earlier test counts.

Raw source and binary identities remain unchanged as historical evidence. Temporary checkouts, compiled probes, and build caches are not committed. The two intermediate source baselines can be reconstructed with `benchmark/build-baseline.py`; restored source hashes are checked before building. Rebuilt binaries may have different hashes because temporary paths, toolchains, and link environments affect binary identity.

Run the summarizers to regenerate the marked table regions from persisted samples:

```sh
python3 scripts/summarize-benchmarks.py
python3 scripts/summarize-jieba-benchmarks.py
python3 scripts/summarize-utf8-benchmarks.py
python3 benchmark/simd-research/analyze.py
python3 benchmark/remaining-simd/summarize.py
```

### Initial static core

<details>
<summary>Original binding versus the first standalone-core implementation</summary>

### Benchmark method

The previous benchmark used a UTF-8 Buffer for the native long input and a JavaScript string for the JavaScript libraries. It also compared native plain arrays with pinyin-pro's default tone-marked string. The replacement uses the same JavaScript input strings, explicit tone settings, grouped non-Han output, and the same array/string output shape. String-to-string comparisons include the old binding's necessary `.join(' ')` step. The JavaScript tone-sandhi option is disabled to reduce policy differences, without claiming that all remaining policies match.

For the Node array, string, and all-reading comparisons, each candidate is warmed before measurement. Seven rounds rotate execution order to distribute drift. Each sample runs for at least 100 ms, with an adaptive batch size to reduce clock overhead on short calls. Outputs are consumed; hashes and sample durations are persisted. Reported minima and maxima describe sample averages, not per-call latency percentiles or confidence intervals. Before timing, default-mode results must equal the saved legacy build. Designated synthetic workloads must also exactly match pinyin-pro. These checks are outside timed sections.

The corpus includes short phrases, mixed text and emoji, ASCII runs, and 1k/10k/100k-character slices or repetitions of the repository's literary text. Additional deterministic randomized workloads use a restricted alphabet whose readings match across implementations. These are explicitly synthetic, low-ambiguity workloads; they complement rather than replace natural text.

Cold-load measurements use fresh processes and report module loading separately from the first contextual conversion. Comparator measurements test short keys, early differences, and long shared prefixes. Asynchronous measurements include the promise round trip and output creation and are checked against each implementation's synchronous result.

<!-- RESULTS -->

### Array output

Runtime: v24.13.1; pinyin-pro 3.29.3. Median microseconds per call, seven rounds. Tone marks and `segment: true` are explicit. String-mode baseline includes joining its array.

| Workload        | Previous Rust | New Rust | pinyin-pro | Speedup vs previous | Speedup vs JS | New output equals JS |
| --------------- | ------------: | -------: | ---------: | ------------------: | ------------: | -------------------- |
| short           |          0.58 |     0.46 |       0.58 |               1.25× |         1.25× | Yes                  |
| mixed           |          0.67 |     0.52 |       1.55 |               1.28× |         2.97× | Yes                  |
| ascii-4k        |         38.89 |     2.69 |     285.31 |              14.46× |       106.05× | Yes                  |
| literature-1k   |         75.47 |    29.83 |      88.53 |               2.53× |         2.97× | No                   |
| literature-10k  |        738.33 |   285.09 |     994.49 |               2.59× |         3.49× | No                   |
| literature-100k |      7,538.96 | 2,979.95 |  12,755.49 |               2.53× |         4.28× | No                   |
| matched-10k     |        913.54 |   389.55 |   1,031.85 |               2.35× |         2.65× | Yes                  |
| matched-100k    |      9,826.45 | 4,708.91 |  12,553.66 |               2.09× |         2.67× | Yes                  |
| phrases         |          1.66 |     1.30 |       2.04 |               1.28× |         1.57× | Yes                  |

[All styles, options, output hashes, and individual samples](../benchmark/results/arrays.json).

### String output

Runtime: v24.13.1; pinyin-pro 3.29.3. Median microseconds per call, seven rounds. Tone marks and `segment: true` are explicit. String-mode baseline includes joining its array.

| Workload        | Previous Rust | New Rust | pinyin-pro | Speedup vs previous | Speedup vs JS | New output equals JS |
| --------------- | ------------: | -------: | ---------: | ------------------: | ------------: | -------------------- |
| short           |          0.66 |     0.28 |       0.65 |               2.37× |         2.34× | Yes                  |
| mixed           |          0.73 |     0.41 |       1.66 |               1.79× |         4.07× | Yes                  |
| ascii-4k        |         39.45 |     2.71 |     285.07 |              14.55× |       105.15× | Yes                  |
| literature-1k   |         91.17 |    13.66 |     107.04 |               6.68× |         7.84× | No                   |
| literature-10k  |        904.96 |   131.81 |   1,205.87 |               6.87× |         9.15× | No                   |
| literature-100k |      9,690.80 | 1,416.92 |  15,623.14 |               6.84× |        11.03× | No                   |
| matched-10k     |      1,077.75 |   191.86 |   1,278.29 |               5.62× |         6.66× | Yes                  |
| matched-100k    |     11,661.05 | 2,648.84 |  15,674.74 |               4.40× |         5.92× | Yes                  |
| phrases         |          1.89 |     0.51 |       2.28 |               3.68× |         4.43× | Yes                  |

[All styles, options, output hashes, and individual samples](../benchmark/results/strings.json).

### All-reading output

Runtime: v24.13.1; pinyin-pro 3.29.3. Median microseconds per call, seven rounds. Tone marks and `segment: true` are explicit. String-mode baseline includes joining its array.

| Workload        | Previous Rust | New Rust | pinyin-pro | Speedup vs previous | Speedup vs JS | New output equals JS |
| --------------- | ------------: | -------: | ---------: | ------------------: | ------------: | -------------------- |
| short           |          0.92 |     0.59 |       0.30 |               1.57× |         0.52× | No                   |
| mixed           |          0.88 |     0.62 |       0.75 |               1.43× |         1.21× | No                   |
| ascii-4k        |         42.06 |     2.71 |      82.12 |              15.54× |        30.34× | Yes                  |
| literature-1k   |        173.53 |    68.71 |      77.72 |               2.53× |         1.13× | No                   |
| literature-10k  |      1,735.71 |   691.69 |     758.42 |               2.51× |         1.10× | No                   |
| literature-100k |     17,982.78 | 7,153.67 |  11,625.23 |               2.51× |         1.63× | No                   |
| matched-10k     |      1,664.18 |   712.17 |     745.02 |               2.34× |         1.05× | No                   |
| matched-100k    |     17,493.43 | 7,236.78 |  11,314.61 |               2.42× |         1.56× | No                   |
| phrases         |          3.41 |     1.57 |       1.25 |               2.17× |         0.79× | No                   |

[All styles, options, output hashes, and individual samples](../benchmark/results/heteronyms.json).

An exact-output “Yes” is a checked equality on these inputs. It is not a general pronunciation-equivalence claim. Natural-text “No” rows compare the same output format with different readings. Heteronym dictionaries also differ in reading coverage and order; very short calls can still favor JavaScript.

### Cold initialization

Median milliseconds in nine fresh processes per implementation. The filesystem cache is not cleared. RSS includes the Node process, input, library, and first result.

| Implementation |  Load | First contextual call | Combined | RSS after first call, MiB |
| -------------- | ----: | --------------------: | -------: | ------------------------: |
| baseline       | 0.810 |                69.098 |   69.912 |                     111.5 |
| rust           | 0.798 |                 0.025 |    0.823 |                      43.6 |
| pinyin-pro     | 9.935 |                 4.588 |   14.525 |                      71.0 |

### Comparison and asynchronous calls

| Operation                                          | Previous, µs | New, µs | Speedup |
| -------------------------------------------------- | -----------: | ------: | ------: |
| common-prefix-sort                                 |       602.36 |  328.46 |   1.83× |
| early-exit-sort                                    |       604.86 |   47.23 |  12.81× |
| short-sort                                         |         0.80 |    0.18 |   4.50× |
| async literary corpus, tone marks, contextual mode |      1689.13 |  799.12 |   2.11× |

[Runtime samples](../benchmark/results/runtime.json). Native comparison still pays for copying JavaScript input strings even when the Rust comparator exits early.

### Large natural-text scaling

Three fresh-process runs per size and implementation, each warmed on the original corpus. Tone-marked arrays and contextual mode are explicit. Readings differ across implementations; these are throughput and memory measurements, not equivalent-accuracy results.

| Characters | Implementation | Median conversion, ms | Median peak RSS, MiB |
| ---------: | -------------- | --------------------: | -------------------: |
|  1,000,000 | baseline       |                 95.82 |                222.3 |
|  1,000,000 | rust           |                 45.48 |                155.0 |
|  1,000,000 | pinyin-pro     |                174.84 |                357.6 |
| 10,000,000 | baseline       |              1,043.71 |              1,529.0 |
| 10,000,000 | rust           |                463.16 |              1,151.7 |
| 10,000,000 | pinyin-pro     |              1,869.11 |              2,109.5 |

[Scaling samples and output hashes](../benchmark/results/scaling.json). Peak RSS includes module loading, the source and expanded input, scratch allocations, and the retained output; it is sampled before the post-timing output hashing.

### Build and core measurements

The fresh native release binary falls from **3,768,512 bytes to 1,746,032 bytes**, a **53.7% reduction**. The older pre-existing artifact was not used for this size comparison.

[Rust-only benchmark samples](../benchmark/results/core.txt) separate lookup and reusable output from contextual processing and owned output. These measurements do not include Node-API or JavaScript allocation.

<!-- END RESULTS -->

</details>

### Jieba integration measurements

<details>
<summary>Optional Jieba integration, cold initialization, and large-input memory</summary>

### Measurement method

All timed comparisons use the same JavaScript strings, plain or tone-marked output, grouped non-Han text, and matching array or string return formats. String output from the original binding includes its necessary `.join(' ')`. pinyin-pro 3.29.3 uses explicit options and `toneSandhi: false`. Jieba uses its default dictionary and `HMM=false`; the Rust resolvers both run with `segment: true`.

The four candidates are the original binding, the current binary's phrase resolver, that same binary's Jieba resolver, and pinyin-pro. Each is warmed, then measured in seven rounds of at least 100 ms with rotating execution order. No input-result memoization is added. Output hashes and individual timing samples are retained. The designated synthetic inputs must produce exactly equal results in all four implementations before timing; natural-text differences are recorded.

Measurements use an Apple M5 Max with 128 GiB RAM and Node 24.13.1 on an active desktop. Builds retain the repository's portable release configuration. Results are workload- and runtime-specific. Fresh-process measurements expose module loading, first contextual conversion, and RSS separately. Async samples include the promise round trip and result creation with an already-created input Buffer. Large-input measurements use three warmed fresh processes per size and implementation; peak RSS includes input, library, scratch storage, and retained output, and is sampled before output hashing.

<!-- jieba-results:start -->

### Arrays

Median microseconds per call, seven rotated rounds. Tone marks and contextual conversion are enabled.

| Workload        |  Original |   Phrase |    Jieba | pinyin-pro | Jieba speedup vs JS | Jieba output equals JS |
| --------------- | --------: | -------: | -------: | ---------: | ------------------: | ---------------------- |
| short           |      0.70 |     0.63 |     0.81 |       0.71 |               0.88× | Yes                    |
| mixed           |      0.87 |     0.72 |     0.99 |       1.90 |               1.92× | Yes                    |
| ascii-4k        |     49.81 |     3.32 |     3.77 |     370.51 |              98.39× | Yes                    |
| literature-1k   |     87.99 |    35.08 |    51.91 |     107.87 |               2.08× | No                     |
| literature-10k  |    937.10 |   351.42 |   541.66 |   1,235.18 |               2.28× | No                     |
| literature-100k |  8,767.19 | 3,519.83 | 5,150.17 |  14,931.39 |               2.90× | No                     |
| matched-10k     |  1,053.84 |   443.79 |   762.40 |   1,191.40 |               1.56× | Yes                    |
| matched-100k    | 10,878.60 | 5,362.00 | 8,648.91 |  14,117.28 |               1.63× | Yes                    |
| phrases         |      1.84 |     1.54 |     1.88 |       2.33 |               1.24× | Yes                    |

On the exact-output 100k synthetic input with plain pinyin, Jieba takes 8.53 ms versus 43.69 ms for pinyin-pro (5.12× faster).

[All options, hashes, and samples](../benchmark/results/jieba/arrays.json).

### Strings

Median microseconds per call, seven rotated rounds. Tone marks and contextual conversion are enabled.

| Workload        |  Original |   Phrase |    Jieba | pinyin-pro | Jieba speedup vs JS | Jieba output equals JS |
| --------------- | --------: | -------: | -------: | ---------: | ------------------: | ---------------------- |
| short           |      0.70 |     0.35 |     0.51 |       0.69 |               1.36× | Yes                    |
| mixed           |      0.76 |     0.48 |     0.74 |       1.74 |               2.36× | Yes                    |
| ascii-4k        |     41.89 |     2.91 |     3.00 |     315.47 |             105.21× | Yes                    |
| literature-1k   |     94.77 |    15.69 |    31.84 |     111.51 |               3.50× | No                     |
| literature-10k  |    948.57 |   151.46 |   299.48 |   1,233.64 |               4.12× | No                     |
| literature-100k | 10,485.11 | 1,607.81 | 3,162.57 |  17,082.06 |               5.40× | No                     |
| matched-10k     |  1,157.66 |   215.82 |   516.16 |   1,257.36 |               2.44× | Yes                    |
| matched-100k    | 12,209.71 | 2,761.62 | 5,874.02 |  15,568.41 |               2.65× | Yes                    |
| phrases         |      1.94 |     0.60 |     0.96 |       2.37 |               2.47× | Yes                    |

On the exact-output 100k synthetic input with plain pinyin, Jieba takes 5.44 ms versus 40.82 ms for pinyin-pro (7.50× faster).

[All options, hashes, and samples](../benchmark/results/jieba/strings.json).

An equality flag refers to the new Jieba result. The synthetic matched workloads assert equality across all four implementations. Natural-text “No” rows compare matching formats with different pronunciations, not equivalent accuracy.

The final phrase and Jieba resolvers differ at **0 of 19,100 output tokens** in the supplied corpus. Its repetitions are not independent accuracy samples. [Differences and pronunciation examples](../benchmark/results/jieba/runtime.json).

### Cold initialization

Medians from nine fresh processes per candidate. The filesystem cache is not cleared. RSS includes Node and the first result.

| Implementation   | Module load, ms | First conversion, ms | Combined, ms | RSS, MiB |
| ---------------- | --------------: | -------------------: | -----------: | -------: |
| Original binding |           0.877 |               72.319 |       73.170 |    111.5 |
| Phrase resolver  |           0.896 |                0.026 |        0.922 |     43.7 |
| Jieba resolver   |           0.875 |               74.028 |       74.992 |    111.7 |
| pinyin-pro       |          10.747 |                4.864 |       15.552 |     71.0 |

Jieba initialization is paid on its first non-ASCII conversion; it is absent from warmed throughput samples. Selecting the phrase resolver does not initialize Jieba.

### Asynchronous conversion

Median milliseconds for the supplied corpus, seven warmed rounds. Input Buffer creation is outside timing; the binding snapshot, queueing, segmentation, formatting, and JS result creation are included.

| Implementation   | End-to-end latency, ms |
| ---------------- | ---------------------: |
| Original binding |                  1.578 |
| Phrase resolver  |                  0.681 |
| Jieba resolver   |                  1.044 |

### Large natural-text scaling

Tone-marked arrays, three fresh-process runs per size and candidate, warmed on the original corpus. Readings differ from pinyin-pro.

| Characters | Implementation   | Median conversion, ms | Median peak RSS, MiB |
| ---------: | ---------------- | --------------------: | -------------------: |
|  1,000,000 | Original binding |                101.66 |                222.2 |
|  1,000,000 | Phrase resolver  |                 48.29 |                155.0 |
|  1,000,000 | Jieba resolver   |                 65.75 |                244.4 |
|  1,000,000 | pinyin-pro       |                186.50 |                357.5 |
| 10,000,000 | Original binding |              1,093.58 |              1,528.8 |
| 10,000,000 | Phrase resolver  |                501.63 |              1,151.8 |
| 10,000,000 | Jieba resolver   |                670.41 |              1,528.1 |
| 10,000,000 | pinyin-pro       |              2,111.87 |              2,109.0 |

[Scaling samples and output hashes](../benchmark/results/jieba/scaling.json).

### Artifact size

The native binary containing both resolvers is 4,131,536 bytes. The pre-integration binary was 1,746,032 bytes; the original binding was 3,768,512 bytes. Optional Rust consumers can disable Jieba entirely; the shipped Node binary includes its dictionary even when callers select the phrase resolver.

<!-- jieba-results:end -->

</details>

### SIMD UTF-8 validation measurements

<details>
<summary>Isolated validators, Rust conversion, complete byte-input calls, and string controls</summary>

### Measurements

Measured on September 11, 2026, on an Apple M5 Max with 128 GiB RAM, macOS ARM64, Rust 1.98.0, and Node v24.13.1 via `oxnode`. Both addons use release optimization, LTO, and one codegen unit. This was an active desktop, not an isolated benchmark host. All medians retain seven samples; execution order rotates each round. Builds and tests did not overlap timed runs, and the benchmark suites ran sequentially.

`benches/utf8.rs` measures only validation with reusable input buffers, 100 ms rounds, a function pointer with black-boxed input/output, and batches of 64 calls. Before timing it compares validity, error offsets, lengths, and display text against Rust's standard library over unaligned slices, truncated sequences, every byte replacement at selected boundaries, and deterministic random bytes. Fixture numbers are target byte sizes, truncated to a valid character boundary where necessary; raw records retain actual lengths. An early invalid sequence still has a few nanoseconds of SIMD setup overhead.

<!-- validation:start -->

| Input             |  std, µs | SIMD compat, µs | Speedup | Basic + std error fallback, µs |
| ----------------- | -------: | --------------: | ------: | -----------------------------: |
| ascii-12          |    0.006 |           0.006 |   1.01× |                          0.005 |
| chinese-12        |    0.006 |           0.006 |   0.99× |                          0.006 |
| mixed-12          |    0.006 |           0.006 |   0.99× |                          0.006 |
| ascii-64          |    0.004 |           0.003 |   1.46× |                          0.002 |
| chinese-64        |    0.023 |           0.023 |   0.99× |                          0.023 |
| mixed-64          |    0.022 |           0.023 |   0.99× |                          0.022 |
| ascii-1024        |    0.019 |           0.007 |   2.53× |                          0.007 |
| chinese-1024      |    0.363 |           0.074 |   4.93× |                          0.071 |
| mixed-1024        |    0.335 |           0.072 |   4.63× |                          0.072 |
| ascii-300000      |    4.712 |           2.366 |   1.99× |                          2.357 |
| chinese-300000    |  102.622 |          21.322 |   4.81× |                         21.397 |
| mixed-300000      |   95.196 |          20.849 |   4.57× |                         20.773 |
| ascii-3000000     |   48.222 |          26.736 |   1.80× |                         26.725 |
| chinese-3000000   | 1064.470 |         207.690 |   5.13× |                        204.641 |
| mixed-3000000     |  961.034 |         209.346 |   4.59× |                        204.282 |
| invalid-start-3m  |    0.002 |           0.006 |   0.39× |                        213.157 |
| invalid-middle-3m |  463.247 |         104.665 |   4.43× |                        668.847 |
| invalid-end-3m    |  964.179 |         213.061 |   4.53× |                       1172.333 |
| truncated-end-3m  |  968.954 |         210.877 |   4.59× |                       1180.391 |

<!-- validation:end -->

#### Rust conversion in the same executable

The separate Node addon builds also differ in paths that bypass validation. To better isolate the validator's contribution, `PINYIN_BENCH_CONVERSION=1 cargo bench --bench utf8` uses one shared, non-inlined conversion function with a black-boxed validator function pointer. Only that pointer changes between samples; allocation, dictionary lookup, optional Jieba segmentation, and owned tone-string output use identical compiled code. Outputs are asserted equal. Each candidate warms up before seven rotating 150 ms rounds. This uses the Rust executable's default allocator and excludes Node/UTF-16 output costs.

<!-- core:start -->

| Input           | Resolver  | std + conversion, µs | SIMD + conversion, µs | Change in time |
| --------------- | --------- | -------------------: | --------------------: | -------------: |
| literature-100k | character |               456.71 |                343.11 |         -24.9% |
| literature-100k | jieba     |              2298.42 |               2185.85 |          -4.9% |
| mixed-100k      | character |               323.60 |                264.65 |         -18.2% |
| mixed-100k      | jieba     |              1316.78 |               1255.53 |          -4.7% |

<!-- core:end -->

`benchmark/utf8.ts` measures complete calls against the saved pre-SIMD addon, including conversion and JS output creation. Each implementation warms up before seven rotating 150 ms rounds. Buffers are pre-encoded outside timing. Options use tone marks, with either default character readings or `segment: true, segmenter: 'jieba'`; both Jieba dictionaries are initialized before timing. Every valid output is compared exactly to the baseline. Invalid inputs compare error code and message. Async timings include buffer copying, worker scheduling, and result creation with one awaited request at a time.

Negative percentages mean less time. These are observed median differences, not confidence intervals. The string controls below bypass the changed validator and show noise or incidental binary-layout effects; their changes must not be attributed to SIMD validation. Single-digit changes need that context.

#### Byte inputs

<!-- buffer:start -->

| Input           | Resolver  | API          | Before, µs | SIMD, µs | Change in time |
| --------------- | --------- | ------------ | ---------: | -------: | -------------: |
| short           | character | pinyin       |       0.77 |     0.77 |          +0.8% |
| short           | character | pinyinString |       0.45 |     0.44 |          -2.5% |
| short           | jieba     | pinyin       |       1.07 |     1.10 |          +2.9% |
| short           | jieba     | pinyinString |       0.70 |     0.69 |          -2.0% |
| ascii-300kb     | character | pinyin       |      52.66 |    48.24 |          -8.4% |
| ascii-300kb     | character | pinyinString |      85.35 |    80.85 |          -5.3% |
| literature-1k   | character | pinyin       |      26.87 |    24.35 |          -9.4% |
| literature-1k   | character | pinyinString |      10.84 |     8.42 |         -22.3% |
| literature-1k   | jieba     | pinyin       |      45.46 |    42.71 |          -6.1% |
| literature-1k   | jieba     | pinyinString |      29.41 |    26.57 |          -9.7% |
| literature-100k | character | pinyin       |    2653.16 |  2449.87 |          -7.7% |
| literature-100k | character | pinyinString |    1090.57 |   859.97 |         -21.1% |
| literature-100k | jieba     | pinyin       |    4590.74 |  4339.38 |          -5.5% |
| literature-100k | jieba     | pinyinString |    3097.90 |  2787.29 |         -10.0% |
| mixed-100k      | character | pinyin       |    1259.76 |  1079.54 |         -14.3% |
| mixed-100k      | character | pinyinString |     613.90 |   487.95 |         -20.5% |
| mixed-100k      | jieba     | pinyin       |    2335.99 |  2139.22 |          -8.4% |
| mixed-100k      | jieba     | pinyinString |    1658.53 |  1517.46 |          -8.5% |
| invalid-start   | —         | pinyinString |       2.70 |     2.72 |          +0.5% |
| invalid-middle  | —         | pinyinString |     545.02 |   115.99 |         -78.7% |
| invalid-end     | —         | pinyinString |    1007.49 |   211.35 |         -79.0% |
| truncated-end   | —         | pinyinString |    1185.51 |   211.49 |         -82.2% |
| literature-100k | character | asyncPinyin  |    2647.61 |  2413.50 |          -8.8% |
| literature-100k | jieba     | asyncPinyin  |    4699.16 |  4437.34 |          -5.6% |

<!-- buffer:end -->

#### JavaScript string controls

<!-- string:start -->

| Input           | Resolver  | API          | Before, µs | SIMD, µs | Change in time |
| --------------- | --------- | ------------ | ---------: | -------: | -------------: |
| short           | character | pinyin       |       0.72 |     0.72 |          +0.8% |
| short           | character | pinyinString |       0.38 |     0.38 |          -2.5% |
| short           | jieba     | pinyin       |       0.99 |     0.99 |          -0.2% |
| short           | jieba     | pinyinString |       0.64 |     0.62 |          -2.2% |
| ascii-300kb     | character | pinyin       |     267.65 |   250.07 |          -6.6% |
| ascii-300kb     | character | pinyinString |     276.13 |   274.33 |          -0.7% |
| literature-1k   | character | pinyin       |      27.97 |    26.58 |          -5.0% |
| literature-1k   | character | pinyinString |      11.89 |    10.59 |         -10.9% |
| literature-1k   | jieba     | pinyin       |      46.64 |    44.62 |          -4.3% |
| literature-1k   | jieba     | pinyinString |      30.24 |    28.64 |          -5.3% |
| literature-100k | character | pinyin       |    2766.23 |  2643.34 |          -4.4% |
| literature-100k | character | pinyinString |    1196.92 |  1071.49 |         -10.5% |
| literature-100k | jieba     | pinyin       |    4607.33 |  4453.74 |          -3.3% |
| literature-100k | jieba     | pinyinString |    3057.28 |  2942.55 |          -3.8% |
| mixed-100k      | character | pinyin       |    1371.00 |  1241.86 |          -9.4% |
| mixed-100k      | character | pinyinString |     721.72 |   654.68 |          -9.3% |
| mixed-100k      | jieba     | pinyin       |    2447.15 |  2320.60 |          -5.2% |
| mixed-100k      | jieba     | pinyinString |    1792.83 |  1720.71 |          -4.0% |
| literature-100k | character | asyncPinyin  |    2878.60 |  2740.90 |          -4.8% |
| literature-100k | jieba     | asyncPinyin  |    4790.18 |  4576.50 |          -4.5% |

<!-- string:end -->

Raw samples, input hashes, source hashes, machine details, and the before/after native artifact hashes are retained in [`benchmark/results/simdutf8`](../benchmark/results/simdutf8). `build.json` also identifies the WASM artifact. The baseline binary was saved after the Jieba stage and before this dependency/helper change. Temporary binary paths in the reports are provenance, not checked-in files.

</details>

### SIMD candidate research

<details>
<summary>Historical isolated kernels and same-binary experiments before UTF-16 integration</summary>

These candidate experiments precede implementation. References to work still to be done describe the decision at that stage; the final path table records what shipped. Isolated kernel timings exclude the rest of conversion.

### Measurement basis

Measurements were collected on September 11, 2026, on an Apple M5 Max, 128 GiB RAM, macOS ARM64, Rust 1.98.0, and Node 24.13.1. The exact V8 version, source hashes, input hashes, addon identity, and raw samples are retained with the results. Release builds use LTO and one codegen unit. No machine-specific compiler flags were added. This was an active desktop, not an isolated or thermally controlled benchmark host.[^simd-1]

The Rust experiments contain **896 timing samples**: 128 fixture/operation/implementation combinations, each measured in seven rotating 100 ms rounds after warmup. They compare owned outputs, including allocation, rather than comparing a scalar allocating operation against a SIMD operation with reusable output. The experimental encoder writes into reserved but uninitialized vector capacity; `encoding_rs`, whose safe API accepts an initialized slice, includes zero initialization. This allocation difference is part of the measured adapter cost, not exclusively an instruction-level comparison.

The Node experiment contains **42 result groups**, also with seven rotating rounds. Scalar and SIMD output conversion are selected in **one addon binary**, with the selector changed outside timed loops. This avoids the different-binary confounding observed in the preceding validation experiment. Existing ASCII output paths serve as controls: the large matched plain-array and plain-string controls differed by less than 0.5% between modes. The experimental global selector is a measurement mechanism and is not a proposed production API.[^simd-1]

Correctness checks compared transcoding over every Unicode scalar, unaligned valid slices, block tails, ASCII control bytes, and escaping boundaries. Node comparisons checked all five styles, all three resolvers, strings and buffers, flat and nested arrays, string and async output, custom separators, controls, emoji, and unpaired JavaScript surrogates against the unchanged addon. These are local oracle checks; cross-architecture execution, new WASM SIMD validation, sanitizers/fuzzing, and remote CI have not been completed for these prototypes.

### 1. SIMD output transcoding

SIMD transcoding groups multiple encoded characters, classifies their byte lengths, and uses shuffle/bit operations to reconstruct code units. Established algorithms specialize common ASCII, two-byte, and three-byte patterns while retaining a general path. This is a good match for long tone-marked pinyin output, but the output's mixture of short ASCII syllables, combining-length patterns, punctuation, and emoji still matters.[^simd-4]

The experiment uses **Rust `simdutf` 0.7.0**, whose bundled C++ implementation identifies itself as **simdutf 7.7.1**. This is distinct from the existing pure-Rust `simdutf8` validator. The binding's public low-level transcoding functions require unsafe pointer calls; the experiment wraps them around valid `&str`, disjoint allocations, conservative capacity bounds, and publication of initialized output only.[^simd-5]

The following measurements isolate output encoding. They do not include dictionary lookup or JavaScript allocation. The `std reserved` control tests whether simply changing capacity accounts for the improvement.

<!-- transcode:start -->

| Input / output           | std collect, µs | std reserved, µs | encoding_rs stable, µs | simdutf, µs |
| ------------------------ | --------------: | ---------------: | ---------------------: | ----------: |
| short / joined           |            0.09 |             0.03 |                   0.04 |        0.03 |
| short / json             |            0.10 |             0.05 |                   0.04 |        0.04 |
| literature-100k / joined |          281.00 |           275.48 |                 299.50 |      185.01 |
| literature-100k / json   |          382.20 |           375.67 |                 316.49 |      273.79 |
| mixed-100k / joined      |          106.44 |           106.54 |                  96.84 |       52.48 |
| mixed-100k / json        |          176.11 |           173.34 |                 102.38 |      125.14 |
| ascii-run / joined       |          189.60 |           184.85 |                  26.15 |        9.00 |
| ascii-run / json         |          189.72 |           185.00 |                  26.13 |        9.13 |

<!-- transcode:end -->

For natural-text joined output, SIMD reduces the encoding step from about **281 to 185 µs**, a **1.52×** speedup. The complete call improves less because encoding is only part of conversion. Reserving scalar output capacity saves little on this fixture. Stable `encoding_rs` is not consistently better: it is slower than the scalar collector on natural joined output, but faster than this simdutf adapter on mixed JSON output.[^simd-1]

The large ASCII encoding result is deliberately retained as a control, not a production claim: the binding already avoids UTF-16 conversion for ASCII output. It would be misleading to present that approximately 21× isolated encoding gain as a pinyin speedup.

#### Complete Node calls and pinyin-pro

All rows below use JavaScript string input and tone-marked output. The table compares the scalar and SIMD modes of the same experimental addon. `pinyin` returns an array; `pinyinString` returns one string.

<!-- node:start -->

| Input / resolver / API                     | Scalar output, µs | SIMD output, µs | Time change | pinyin-pro, µs |
| ------------------------------------------ | ----------------: | --------------: | ----------: | -------------: |
| literature-100k / character / pinyin       |           2757.86 |         2589.91 |       -6.1% |              — |
| literature-100k / character / pinyinString |           1048.56 |          937.89 |      -10.6% |              — |
| literature-100k / jieba / pinyin           |           4695.22 |         4529.87 |       -3.5% |       13547.62 |
| literature-100k / jieba / pinyinString     |           2889.51 |         2802.60 |       -3.0% |       15739.32 |
| mixed-100k / character / pinyin            |           1492.57 |         1413.96 |       -5.3% |              — |
| mixed-100k / character / pinyinString      |            771.89 |          731.47 |       -5.2% |              — |
| mixed-100k / jieba / pinyin                |           2382.02 |         2334.19 |       -2.0% |              — |
| mixed-100k / jieba / pinyinString          |           1771.62 |         1746.23 |       -1.4% |              — |
| matched-100k / character / pinyin          |           3638.69 |         3267.40 |      -10.2% |       12611.32 |
| matched-100k / character / pinyinString    |           1655.46 |         1347.63 |      -18.6% |       14656.09 |
| matched-100k / jieba / pinyin              |           7704.37 |         7157.34 |       -7.1% |       12590.06 |
| matched-100k / jieba / pinyinString        |           5637.06 |         5240.79 |       -7.0% |       14936.88 |

<!-- node:end -->

The matched corpus asserts identical outputs against **pinyin-pro 3.29.3**, with tone sandhi disabled and consecutive non-Chinese runs grouped. On that corpus, SIMD-output Jieba conversion is **1.76×** as fast as pinyin-pro for arrays and **2.85×** for strings. The natural-text pinyin-pro outputs differ in readings; those rows describe workloads, not equivalent accuracy. No general accuracy conclusion follows from the matched synthetic corpus.[^simd-1][^simd-6]

**Implementation recommendation:** add an internal output-transcoding adapter in the Node binding, retaining the existing ASCII path and short-array cutoff. Keep the standalone core's default dependencies unchanged. Benchmark the crossover on each supported architecture instead of assuming a fixed width or threshold from this M5 Max. Retain a portable fallback and test UTF-16 surrogate pairs, NULs, allocation bounds, and worker ownership before adoption.

The Rust binding builds C++ and links its standard library. The local probe links `libc++.1.dylib` and is 4,300,576 bytes versus the production addon's 4,131,808 bytes; that 168,768-byte difference includes experimental controls and is not an exact shipping-size prediction. Packaging and toolchain support need checking across this project's native and WASI targets. `encoding_rs` 0.8.41 is a viable pure-Rust comparison, but its explicit SIMD acceleration feature still depends on nightly functionality; its ordinary stable configuration is what was measured here. Stable `std::simd` is not yet available. Architecture-specific intrinsics or an encapsulated backend avoid requiring nightly for the whole project.[^simd-1][^simd-5][^simd-7][^simd-8]

### 2. Direct UTF-16 output and input

#### Remove output transcoding for known syllables

The dictionary already precomputes five output styles. Precomputing corresponding UTF-16 syllable slices would let Node-oriented writers append code units directly. That eliminates the UTF-8 output buffer and subsequent full encoding pass for mapped syllables; SIMD would then be useful mainly for unchanged non-Han runs. This is a reduction in work, rather than a SIMD acceleration of the existing work.[^simd-2]

The prototype builds its cache outside timing and reuses it, representing a future generated static table. It writes directly from the existing token stream and uses scalar UTF-16 encoding for unchanged runs. It does not modify pronunciation selection.

<!-- direct:start -->

| Input / resolver            | Current string preparation, µs | SIMD transcode, µs | Direct cached UTF-16, µs |
| --------------------------- | -----------------------------: | -----------------: | -----------------------: |
| literature-100k / character |                         605.51 |             516.23 |                   447.88 |
| literature-100k / phrase    |                         950.36 |             853.69 |                   760.03 |
| literature-100k / jieba     |                        2400.48 |            2294.40 |                  2204.56 |
| mixed-100k / character      |                         363.77 |             312.95 |                   264.24 |
| mixed-100k / phrase         |                         578.23 |             566.75 |                   467.47 |
| mixed-100k / jieba          |                        1351.89 |            1335.90 |                  1234.09 |

<!-- direct:end -->

For 100,000 natural-text characters, direct UTF-16 output reduces character-mode Rust preparation from **605.51 to 447.88 µs**, versus **516.23 µs** with SIMD transcoding. The direct approach also wins for phrase and Jieba output. These measurements exclude the Node boundary and use the Rust executable's default allocator; they must not be substituted into the Node table as measured total-call times.[^simd-1]

A production implementation should generate tightly sized tables at build time, support all five styles, and retain a dedicated ASCII writer. It should avoid the research prototype's oversized runtime cache and initialization scan. Flat JSON and heteronym JSON need their own UTF-16 writers, with correct escaping and fresh nested arrays. Rust callers expecting UTF-8 `String` should continue to use the existing writer.

#### Avoid the input UTF-8 round trip

N-API supports copying JavaScript strings as UTF-16. Its UTF-16 extraction API reports code-unit length and copies into a caller-provided buffer; it does not expose a general zero-copy borrowed view. The currently used UTF-8 extraction instead measures an encoded byte length and writes UTF-8 into a separate allocation. The local `napi` implementation makes both API calls.[^simd-3][^simd-9]

<!-- input:start -->

| Input           | N-API UTF-8 input extraction, µs | N-API UTF-16 input extraction, µs |
| --------------- | -------------------------------: | --------------------------------: |
| short           |                             0.06 |                              0.04 |
| literature-1k   |                             2.50 |                              0.07 |
| literature-100k |                           257.02 |                              3.46 |
| mixed-100k      |                           172.21 |                              3.31 |
| matched-100k    |                           228.19 |                              3.24 |

<!-- input:end -->

For natural text, the measured difference is about **254 µs**. If that cost could be removed without changing any other work, the current 1,048.56 µs character-mode Node string call would fall by roughly **24%**. This is an optimistic component-based estimate, not a measured new API path: a real implementation must consume UTF-16, maintain output ranges, and perform any required conversion for downstream components.[^simd-1]

The best first candidate is character mode: process BMP code units directly, recognize surrogate pairs correctly, map syllables, and preserve the current replacement behavior for unmatched surrogates. Phrase mode could decode once to scalar values while maintaining UTF-16 offsets. Jieba currently consumes `&str`, so eagerly copying UTF-16 and then converting it back to UTF-8 may lose the benefit. Keep that decision resolver-specific until measured.

A UTF-16-native path could also exploit SIMD to identify blocks containing surrogate code units or classify broad ranges. Broad CJK membership alone must not determine whether a character is converted: the dictionary includes extensions and unmapped characters whose exact lookup semantics must remain intact.

### 3. Bulk input decoding

The default streaming character path avoids a scalar-vector allocation. Phrase and Jieba adapters already allocate `Vec<char>`, making them more plausible consumers of SIMD UTF-8 → UTF-32 decoding. The measured SIMD path first counts scalar values, allocates the exact number of output entries, and then transcodes. Both passes are included.

<!-- decode:start -->

| Input           | Scalar decode, µs | SIMD count + decode, µs | SIMD time change |
| --------------- | ----------------: | ----------------------: | ---------------: |
| short           |              0.05 |                    0.03 |           -41.8% |
| literature-1k   |              0.86 |                    0.31 |           -64.2% |
| literature-100k |             78.31 |                   31.38 |           -59.9% |
| mixed-100k      |             45.42 |                   67.70 |           +49.1% |
| ascii-run       |            142.36 |                   26.12 |           -81.7% |

<!-- decode:end -->

The Chinese result improves **78.31 → 31.38 µs**, saving about **47 µs** per 100,000 characters for one decoded vector. That is useful but much smaller than the whole Jieba call. The mixed fixture regresses **45.42 → 67.70 µs**. These results rule out an unconditional replacement of every `.chars()` iterator with bulk SIMD decoding.[^simd-1]

A bounded three-byte fast path is a candidate for Chinese-heavy blocks. On ARM, a structure load can deinterleave successive byte triples; masking and shifting can reconstruct several BMP scalars together. General UTF-8 still requires fallbacks for ASCII, two-byte characters, four-byte characters, short tails, and block boundaries. The deinterleaving instruction is available, but no custom decoder of this design was benchmarked here.[^simd-10]

The more valuable integration may be **reusing one decoded representation** between phrase selection and output. Otherwise a faster first decode leaves the second scalar decode in place. Keep the lazy, allocation-free character path for small inputs. For byte inputs, preserve validation and its diagnostic offsets; for already-valid Rust strings, avoid adding another validation pass merely to invoke a transcoder.

### 4. JSON escaping and unchanged runs

Only unchanged tokens need JSON escaping. Dictionary syllables are validated as JSON-safe during generation and are already appended directly. The current unchanged-token loop decodes every character even though only ASCII quotes, backslashes, and bytes below 0x20 require escaping. A byte-oriented scanner can skip blocks and append the intervening UTF-8 slice intact.[^simd-2]

This is a proven SIMD pattern: V8's published stringifier work uses hardware SIMD for longer strings and word-level parallel operations for shorter strings. That work supports the scan-and-copy design; its reported `JSON.stringify` improvement is not a performance claim about this crate or its separate `JSON.parse` call.[^simd-11]

The local prototype scans 16 bytes with NEON, uses comparisons for the two special bytes and control-byte range, and falls back to scalar handling at the next escape or tail. It never loads outside the slice. The table measures its additional effect after SIMD transcoding has already been enabled.

<!-- escape:start -->

| Input / resolver            | SIMD transcode only, µs | SIMD transcode + escape scan, µs | Additional time change |
| --------------------------- | ----------------------: | -------------------------------: | ---------------------: |
| literature-100k / character |                  786.68 |                           795.98 |                  +1.2% |
| literature-100k / jieba     |                 2546.40 |                          2544.64 |                  -0.1% |
| mixed-100k / character      |                  527.70 |                           485.00 |                  -8.1% |
| mixed-100k / jieba          |                 1510.05 |                          1440.46 |                  -4.6% |

<!-- escape:end -->

On the mixed fixture, the extra scanner saves **8.1%** of character-mode Rust JSON preparation and **4.6%** with Jieba. On natural text, its effect is approximately neutral; the character-mode row is slightly slower. An isolated long unchanged ASCII run improves **521.96 → 16.37 µs**, but a wholly ASCII call never reaches the production bulk-JSON path. That synthetic result indicates where the scanner works, not a 32× complete-call gain.[^simd-1]

Prioritize mixed documents with long unchanged spans, links, identifiers, or embedded text, and benchmark those workloads before adopting a threshold. A plain `memchr2` search for quotes and backslashes misses control bytes, so it is not a complete replacement. The `memchr` crate is a useful cross-platform search reference, but the needed predicate also includes a byte range.[^simd-12]

### 5. Dictionary and phrase traversal

The common-character table has 20,992 packed 32-bit entries, or **82 KiB**. Lookup is already a direct load for that range. SIMD arithmetic can batch decoding and bounds classification, but syllable selection still requires indexed memory accesses. ARM's multi-register byte table lookup operates on register-held tables; it cannot directly replace an arbitrary 82 KiB lookup. A layout that helps one SIMD architecture may add overhead on another.[^simd-2][^simd-13]

The phrase trie contains **8,960 nodes**. Of its 8,959 non-root nodes, **4,019 have no children** and **4,145 have one child**: together, **91.1%**. A source-equivalent traversal trace over the natural corpus requested 66,258 child searches; 31,028, or **46.8%**, involved 2–4 children. Static node counts alone therefore understate the possible relevance of small multi-edge comparisons.[^simd-1]

A defensible experiment would pack or separately store the code points for 2–4 edges, compare them together, and extract the matching lane. It must preserve valid edge masks, deterministic lookup, and the existing binary-search behavior for large fanouts. Padding all nodes to a wide vector would increase working-set size and could erase the benefit. No timing result for this candidate is established here; the trace counts are not CPU profile percentages.

The backward phrase dynamic program uses future scores and dictionary-dependent transitions. Adjacent positions are not independent vector lanes. Its six-slot rolling cost buffer is already small. Retaining the sequential solver while improving decode reuse or edge comparisons is a better-supported direction than rewriting the entire recurrence for SIMD.[^simd-2]

### 6. Jieba and JavaScript limits

Standalone Jieba cutting measured about **1.38 ms** on 100,000 natural-text characters and **0.72 ms** on the mixed fixture, before this crate's additional phrase selection and output preparation. Jieba 0.10.3 constructs a sparse dictionary graph, follows byte-prefix matches, and evaluates dependent routes. It already stores log frequencies rather than recomputing a logarithm in the innermost route loop. A proposed optimization must target the current version, not assume that older costs remain.[^simd-1][^simd-3]

Possible upstream work includes reusing scratch allocations, reducing repeated decoding during class splitting/token accounting, or accepting a reusable indexing representation. These require separate measurement and API design. SIMD should not replace dictionary decisions with a simple longest-word heuristic: that would change readings and segmentation rather than provide a matched optimization.

Large arrays also pay for engine allocation and parsing. Parsing already-materialized JSON alone measured **1.45 ms** for the natural-text array and **1.81 ms** for the matched array. These are separate hot-input measurements, not additive profiles of the complete native call. They nevertheless show why a 1.5× encoder improvement produces a much smaller array-call improvement.[^simd-1]

Replacing V8 parsing with a Rust SIMD JSON parser would still require constructing the JavaScript array and strings afterward. The existing bulk path was introduced to avoid expensive per-element native calls. A new parser does not eliminate that boundary. For callers that need one string, the existing `pinyinString` API remains an important way to avoid the array's unavoidable work.[^simd-2]

</details>

### UTF-16 integration measurements

<details>
<summary>Complete calls, matching pinyin-pro outputs, startup, memory, and binary size</summary>

This is the stage immediately before the final SIMD pass. Full-module inspection later established that the standard WASM artifact already requires SIMD128; a scalar fallback in our kernels is not a SIMD-free module guarantee.

### Complete conversion calls

Apple M5 Max, 128 GiB RAM, macOS ARM64, Node 24.13.1, Rust 1.98.0; ordinary release builds with LTO and no machine-specific CPU flags. The natural fixture contains 100,000 JavaScript code units from the repository's literature corpus. Times are medians in microseconds; lower is better. These rows use JavaScript string input and tone marks.

| Resolver  | API            | Before µs | After µs | Less time |
| --------- | -------------- | --------- | -------- | --------- |
| character | `pinyin`       | 2579.49   | 2095.31  | 18.8%     |
| character | `pinyinString` | 999.94    | 560.85   | 43.9%     |
| phrase    | `pinyin`       | 2877.49   | 2366.53  | 17.8%     |
| phrase    | `pinyinString` | 1318.91   | 865.69   | 34.4%     |
| jieba     | `pinyin`       | 4314.64   | 3766.78  | 12.7%     |
| jieba     | `pinyinString` | 2704.09   | 2265.21  | 16.2%     |

The byte-input results include UTF-8 validation and preserve borrowing for synchronous calls. They confirm that improvements extend beyond JavaScript input extraction.

| Resolver  | API            | Before µs | After µs | Less time |
| --------- | -------------- | --------- | -------- | --------- |
| character | `pinyin`       | 2345.67   | 2063.98  | 12.0%     |
| character | `pinyinString` | 814.61    | 563.63   | 30.8%     |
| phrase    | `pinyin`       | 2684.86   | 2387.45  | 11.1%     |
| phrase    | `pinyinString` | 1106.83   | 846.44   | 23.5%     |
| jieba     | `pinyin`       | 4047.54   | 3745.28  | 7.5%      |
| jieba     | `pinyinString` | 2501.86   | 2239.15  | 10.5%     |

Mixed text includes Chinese, ASCII, accented Latin text, emoji, and embedded NULs. The fixture has 100,000 JavaScript code units, including surrogate pairs; it is not 100,000 Unicode scalars.

| Resolver  | API            | Before µs | After µs | Less time |
| --------- | -------------- | --------- | -------- | --------- |
| character | `pinyin`       | 1201.47   | 838.62   | 30.2%     |
| character | `pinyinString` | 607.52    | 379.58   | 37.5%     |
| phrase    | `pinyin`       | 1432.10   | 1029.45  | 28.1%     |
| phrase    | `pinyinString` | 829.71    | 598.39   | 27.9%     |
| jieba     | `pinyin`       | 2177.80   | 1805.03  | 17.1%     |
| jieba     | `pinyinString` | 1592.72   | 1371.77  | 13.9%     |

### Comparison with pinyin-pro

The deterministic 100,000-character synthetic fixture uses an alphabet whose readings match across implementations. Every measured output is checked before timing. `pinyin-pro` 3.29.3 uses `toneSandhi: false`, `nonZh: 'consecutive'`, and the corresponding string/array and tone options.

| Resolver  | API            | Rust before µs | Rust after µs | pinyin-pro µs | Speedup vs pro |
| --------- | -------------- | -------------- | ------------- | ------------- | -------------- |
| character | `pinyin`       | 3536.35        | 2736.62       | 11973.90      | 4.38×          |
| character | `pinyinString` | 1616.05        | 889.89        | 13861.11      | 15.58×         |
| phrase    | `pinyin`       | 4433.96        | 3611.93       | 11925.27      | 3.30×          |
| phrase    | `pinyinString` | 2477.74        | 1719.32       | 13968.44      | 8.12×          |
| jieba     | `pinyin`       | 7379.94        | 6595.83       | 11949.63      | 1.81×          |
| jieba     | `pinyinString` | 5424.08        | 4719.77       | 14044.30      | 2.98×          |

These are equal-output comparisons on this fixture, not a claim of identical language features or equal pronunciation accuracy on arbitrary text. Natural text has dictionary and policy differences: the raw results explicitly mark those pinyin-pro comparisons as `matchesPro: false`. The complete dataset also includes plain output; no tones, word boundaries, or output allocation are omitted to obtain a headline ratio.

### Short calls, ASCII, async, and heteronyms

| Fixture     | API            | Input  | Style | Before µs | After µs | Less time |
| ----------- | -------------- | ------ | ----- | --------- | -------- | --------- |
| short       | `pinyin`       | string | plain | 0.62      | 0.64     | -2.2%     |
| short       | `pinyinString` | string | plain | 0.30      | 0.29     | 2.2%      |
| short       | `pinyin`       | string | tone  | 0.68      | 0.63     | 6.4%      |
| short       | `pinyinString` | string | tone  | 0.35      | 0.31     | 12.1%     |
| ascii-300kb | `pinyin`       | string | tone  | 173.85    | 64.84    | 62.7%     |
| ascii-300kb | `pinyin`       | buffer | tone  | 41.69     | 41.18    | 1.2%      |
| ascii-300kb | `pinyinString` | string | tone  | 178.58    | 68.04    | 61.9%     |
| ascii-300kb | `pinyinString` | buffer | tone  | 48.65     | 48.38    | 0.6%      |

Sub-microsecond results should be read in absolute time as well as percentages. The initial implementation added a redundant scan on large ASCII buffers and extra allocations for short Chinese strings; both were removed after measurement. The final dataset retains before/after samples for the unchanged ASCII byte-input routes as controls.

| Fixture         | Resolver  | API           | Input  | Before µs | After µs | Less time |
| --------------- | --------- | ------------- | ------ | --------- | -------- | --------- |
| literature-100k | character | `heteronym`   | string | 6989.14   | 5991.27  | 14.3%     |
| mixed-100k      | character | `heteronym`   | string | 2098.40   | 1649.34  | 21.4%     |
| literature-100k | character | `asyncPinyin` | string | 2600.17   | 2147.14  | 17.4%     |
| literature-100k | character | `asyncPinyin` | buffer | 2414.80   | 2137.35  | 11.5%     |
| literature-100k | jieba     | `asyncPinyin` | string | 4366.16   | 3873.91  | 11.3%     |
| literature-100k | jieba     | `asyncPinyin` | buffer | 4163.05   | 3851.49  | 7.5%      |

Async measurements await calls sequentially and include input ownership, worker scheduling, conversion, and creation of JavaScript arrays. They are latency measurements, not a multi-worker throughput benchmark. Heteronym measurements retain all alternate readings and fresh nested arrays.

### Startup, memory, and binary size

Each cold-start sample uses a fresh process. There are nine samples per implementation and resolver, rotated in order. Load time excludes process startup; the first conversion includes lazy Jieba initialization when selected.

| Resolver  | Load before ms | Load after ms | First call before ms | First call after ms |
| --------- | -------------- | ------------- | -------------------- | ------------------- |
| character | 0.745          | 0.768         | 0.014                | 0.014               |
| phrase    | 0.739          | 0.766         | 0.020                | 0.021               |
| jieba     | 0.753          | 0.796         | 66.880               | 66.350              |

The memory experiment uses five fresh processes per implementation, resolver, and API, with one million JavaScript code units of natural text. It initializes the resolver first, constructs the input, runs GC, then measures one complete conversion. Peak RSS is process-wide, recorded before output hashing; it is not a count of Rust allocations or a claim about every workload.

| Resolver  | API            | Time before ms | Time after ms | Peak before MiB | Peak after MiB |
| --------- | -------------- | -------------- | ------------- | --------------- | -------------- |
| character | `pinyin`       | 41.96          | 36.56         | 154.1           | 135.0          |
| character | `pinyinString` | 11.24          | 6.30          | 79.9            | 68.6           |
| phrase    | `pinyin`       | 44.34          | 38.58         | 156.4           | 135.4          |
| phrase    | `pinyinString` | 13.96          | 9.31          | 80.2            | 75.8           |
| jieba     | `pinyin`       | 60.04          | 54.47         | 246.4           | 229.7          |
| jieba     | `pinyinString` | 29.61          | 25.42         | 169.0           | 172.1          |

Peak RSS fell in five of the six measured cases. Jieba string output increased by approximately 3.1 MiB on this fixture; the latency improvement therefore has a small memory cost in that case.

The native addon grew from 4,131,808 to 4,481,808 bytes (8.5%). The WASI module grew from 3,434,140 to 3,541,152 bytes (3.1%). These are release artifacts; the extra code and static tables are a deliberate binary-size tradeoff.

`simdutf` adds a C++11 toolchain requirement and C++ standard-library linkage for source builds of the native ARM64/x64 binding. The default `simd` feature can be disabled with Cargo's `--no-default-features`. WASM and other CPU architectures use the portable encoding and scanning fallbacks and do not build this dependency. The core crate itself remains safe Rust with no default runtime dependencies. [Compilation requirements](https://docs.rs/simdutf/0.7.0/simdutf/#compilation).

No global AVX, AVX-512, or native-CPU flags are required. The custom scanner uses architecture-baseline instructions, while the transcoding library selects supported native implementations. The pinyin kernels in this historical standard WASM build did not enable SIMD128; full linked modules nevertheless contain SIMD instructions. [simdutf portability and runtime detection](https://github.com/simdutf/simdutf#requirements).

</details>

### json-escape-simd evaluation

<details>
<summary>Five-mode prototype, complete-call results, and why UTF-16 kept its own writer</summary>

This isolated prototype established the UTF-8 integration used in the final binding. Its samples are separate from the final combined implementation.

### Complete-call results

Apple M5 Max, macOS ARM64, Node 24.13.1, release Rust with LTO. Each row is a median of seven 75 ms samples per mode, with order rotated between rounds. Input extraction, resolution, JSON construction, conversion to an engine string, JSON parsing and array allocation are included. These are warm calls on an active desktop, not isolated CPU throughput. Differences of a few percent should be treated cautiously: unchanged control paths also fluctuate.

These rows use plain output, flat arrays and JavaScript string input. The comparison is against the saved pre-final-SIMD Rust implementation, which already includes Jieba, simdutf8 and SIMD transcoding.

| Input                                        | Resolver  | Existing ms | Crate ms | Less time |
| -------------------------------------------- | --------- | ----------: | -------: | --------: |
| Literature, 100,000 UTF-16 units             | Character |       2.221 |    2.211 |      0.4% |
| Literature, 100,000 UTF-16 units             | Jieba     |       3.867 |    3.836 |      0.8% |
| Mixed text, no control characters            | Character |       0.881 |    0.794 |      9.9% |
| Mixed text, no control characters            | Jieba     |       1.772 |    1.696 |      4.3% |
| Mixed text including NUL                     | Character |       0.989 |    0.827 |     16.4% |
| Mixed text including NUL                     | Jieba     |       1.949 |    1.762 |      9.6% |
| Long ASCII run after 32 Chinese characters   | Character |       0.282 |    0.175 |     38.0% |
| Long Unicode run after 32 Chinese characters | Character |       0.493 |    0.329 |     33.3% |

The large-run fixtures deliberately force the bulk-array route with 32 initial Chinese tokens. They are diagnostic workloads, not representative Chinese prose. Pure ASCII input returns one token and bypasses this JSON writer. The clean mixed fixture has 100,000 UTF-16 units; its NUL-containing counterpart has 105,000. Source strings, sizes, hashes and run counts are persisted with the measurements.

### Where the gain comes from

The prototype baseline UTF-8 array writer iterates through non-dictionary text as Unicode characters, checks each character for JSON escaping and appends it individually. Known syllables are already JSON-safe and appended directly. The crate combines scanning, copying unchanged bytes and table-based escape emission. Its ARM64 kernel processes four 16-byte vectors together, combining their masks before extracting a bitmask. [Published NEON implementation](https://github.com/napi-rs/json-escape-simd/blob/a860920ee22a75d3b4983c987ce5c2e0fe1837ae/src/simd/neon.rs).

This is useful even for short mixed-text runs. Every unmapped run in the mixed fixtures is shorter than 64 bytes; restricting the crate to long runs loses their improvement. The literature fixture has 12,410 unmapped runs, mostly short punctuation, and only five escapable bytes. Its total call cost barely changes.

Reusing our existing SIMD finder and copying UTF-8 spans in bulk is a competitive dependency-free alternative: it takes 0.172 ms for the long ASCII case and 0.328 ms for the long Unicode case. The crate is better on short mixed runs: 0.794 versus 0.813 ms without controls, and 0.827 versus 0.873 ms with NUL. The improvement is therefore not exclusively a consequence of wider SIMD; avoiding character decoding and general-purpose control-character formatting also matters.

The public escape_into API appends quoted UTF-8 JSON into a byte vector. It reserves six times the input length plus 35 bytes of spare capacity for worst-case escaping and speculative stores. It exposes neither a UTF-16 sink nor a public escape-finder API. Default backends include NEON, AVX2 and SSE2; AVX-512 is opt-in, and other architectures use the portable implementation. [Published API and dispatch](https://github.com/napi-rs/json-escape-simd/blob/a860920ee22a75d3b4983c987ce5c2e0fe1837ae/src/lib.rs).

### Decision carried into production

The final implementation uses the crate for non-dictionary tokens in the UTF-8 bulk-array writer, covering plain, initials and numeric styles. Keep known syllables on their direct-copy path. Do not apply a 64-byte minimum: that misses the mixed-text benefit.

Keep the direct UTF-16 tone writer for normal workloads. A prototype that escapes non-dictionary text into a reusable UTF-8 scratch buffer and then transcodes it shows no consistent material improvement on prose or ordinary mixed text. There is an exception: one deliberately escape-heavy long-run fixture improves from 0.673 to 0.589 ms, about 12.5%. That result does not justify routing all tone output through an intermediate buffer.

The pinyinString API does not construct JSON, so this crate does not address its costs. It also cannot remove the JavaScript array allocation and parsing that remain after escaping.

### Validation and reproduction

Five modes share one compiled addon: existing behavior, crate for UTF-8, crate for both encodings, our scanner for UTF-8, and crate only for long UTF-8 runs. The mode switch occurs outside timing. The prototype passes 60,800 output comparisons against the production addon across all styles and resolvers, strings and buffers, heteronyms, custom separators, async calls, controls, lone surrogates and multibyte tails. Benchmark fixtures are also compared in every mode. These are functional comparisons on ARM64, not an audit of the dependency's unsafe code or validation on other architectures.

Build with `python3 benchmark/json-escape/build-probe.py`, then run `oxnode benchmark/json-escape/node.ts`. The builder copies sources to a temporary directory and adds the dependency there. It records source and artifact hashes. It never changes the production manifest or replaces the production addon.

- [Initial 22-case measurements](../benchmark/results/json-escape/node.json), with the exact [initial harness snapshot](../benchmark/results/json-escape/harness-initial.txt).
- [Four additional clean-mixed measurements](../benchmark/results/json-escape/clean.json), run with `BENCH_FILTER='mixed-clean/' BENCH_OUTPUT=benchmark/results/json-escape/clean.json oxnode benchmark/json-escape/node.ts`.
- [All cases in CSV](../benchmark/results/json-escape/summary.csv) and [build provenance](../benchmark/results/json-escape/build.json).

The benchmarked crate is published version 3.1.1, revision a860920ee22a75d3b4983c987ce5c2e0fe1837ae. The supplied DeepWiki page was unavailable to the browser tool; the assessment uses the repository and published crate source directly.

</details>

## Sources

[^old-binding]: Brooooooklyn/pinyin, [baseline binding source](https://github.com/Brooooooklyn/pinyin/blob/f4409a36802672f6677a8033d63109b16a8a2f40/src/lib.rs), inspected baseline revision.

[^old-core]: mozillazg/rust-pinyin, [block lookup and data structures](https://github.com/mozillazg/rust-pinyin/blob/76fc7d30de40c85f4cc7e8e138da677a172a78b4/src/lib.rs), pinyin 0.11.0 source revision.

[^old-iterator]: mozillazg/rust-pinyin, [character and string conversion](https://github.com/mozillazg/rust-pinyin/blob/76fc7d30de40c85f4cc7e8e138da677a172a78b4/src/pinyin.rs), pinyin 0.11.0 source revision.

[^pro-utils]: zh-lx/pinyin-pro, [FastDictFactory](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/common/utils.ts), inspected 3.29.3 source revision.

[^pro-entry]: zh-lx/pinyin-pro, [pinyin options and processing pipeline](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/core/pinyin/index.ts).

[^pro-ac]: zh-lx/pinyin-pro, [Aho–Corasick matcher](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/common/segmentit/index.ts).

[^pro-probability]: zh-lx/pinyin-pro, [maximum-probability selection](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/common/segmentit/max-probability.ts) and [probability constants](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/common/constant.ts).

[^pro-speed]: zh-lx/pinyin-pro, [upstream speed benchmark](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/scripts/benchmark/speed.ts), explicit reverse-max-match setting.

[^pro-handle]: zh-lx/pinyin-pro, [pronunciation handling](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/core/pinyin/handle.ts).

[^pro-middleware]: zh-lx/pinyin-pro, [output middleware](https://github.com/zh-lx/pinyin-pro/blob/14b898d6aff00c9df75be351667bcc4155da395b/packages/pinyin-pro/lib/core/pinyin/middlewares.ts).

[^node-api]: Node.js, [Node-API environment and value lifetime documentation](https://nodejs.org/api/n-api.html#environment-life-cycle-apis).

Dictionary originals, source hashes, and license notices are in [the provenance manifest](../crates/pinyin-core/data/sources.json). Measurements are in [benchmark/results](../benchmark/results); build identities are in [build.json](../benchmark/results/build.json). The implementation is in [the standalone core](../crates/pinyin-core/src/lib.rs) and [the Node binding](../src/lib.rs).

[^simd-1]: Local measurements and static analysis, September 11, 2026. [Raw samples and metadata](../benchmark/results/simd-research), [Rust kernels](../benchmark/simd-research/src/main.rs), [Node benchmark](../benchmark/simd-research/node.ts), and [analysis script](../benchmark/simd-research/analyze.py). Source/artifact hashes and validation scope are retained in the metadata.

[^simd-2]: Repository source pointers (the measured historical source hashes are retained in metadata): [Node binding](../src/lib.rs), [core and phrase solver](../crates/pinyin-core/src/lib.rs), [Jieba adapter](../crates/pinyin-core/src/jieba.rs), and [dictionary generator](../crates/pinyin-core/build.rs). The earlier [algorithm report](performance-research.md), [Jieba report](performance-research.md#jieba-integration), and [UTF-8 validation report](performance-research.md#utf-8-validation) document prior stages; their samples are not pooled with this experiment.

[^simd-3]: Cargo-resolved primary sources: `napi` 3.12.3, `src/bindgen_runtime/js_values/string.rs`; `jieba-rs` 0.10.3, `src/lib.rs`. Inspected from the local Cargo registry. Exact identities and source hashes are recorded under `dependency_sources` in [metadata.json](../benchmark/results/simd-research/metadata.json); versions are pinned in the [root lockfile](../Cargo.lock).

[^simd-4]: Daniel Lemire. [Unicode at Gigabytes per Second](https://arxiv.org/html/2111.08692v3), revised May 20, 2023, especially §4. The paper's hardware comparisons motivate the algorithm; only this repository's local measurements supply the performance numbers above.

[^simd-5]: Nugine, [simdutf Rust binding 0.7.0](https://docs.rs/simdutf/0.7.0/simdutf/), compilation and unsafe conversion APIs; [bundled header](https://github.com/Nugine/simdutf-rs/blob/v0.7.0/cpp/simdutf.h); simdutf project, [7.7.1 documentation](https://github.com/simdutf/simdutf/blob/v7.7.1/README.md). Accessed September 11, 2026.

[^simd-6]: zh-lx and contributors. [pinyin-pro](https://github.com/zh-lx/pinyin-pro). Measured installed version 3.29.3; package code hash and exact comparison options are retained in the Node results and benchmark source.

[^simd-7]: Henri Sivonen and contributors. [encoding_rs 0.8.41 README](https://github.com/hsivonen/encoding_rs/blob/v0.8.41/README.md), SIMD feature, platform support, and release notes. Both the current registry source and versioned upstream documentation were inspected; older 0.8.35 search results were not used to infer the current implementation.

[^simd-8]: Rust project. [`std::simd` documentation](https://doc.rust-lang.org/std/simd/index.html), nightly-only status; [stable SIMD RFC](https://rust-lang.github.io/rfcs/2325-stable-simd.html), target-specific functions and runtime dispatch. Accessed September 11, 2026.

[^simd-9]: Node.js. [Node 24.13.1 Node-API documentation](https://nodejs.org/download/release/v24.13.1/docs/api/n-api.html#napi_get_value_string_utf16), UTF-8/UTF-16 string extraction and creation APIs.

[^simd-10]: Rust/Arm intrinsic documentation. [`vld3q_u8`](https://doc.rust-lang.org/stable/core/arch/aarch64/fn.vld3q_u8.html), structure loads into three registers. This supports the proposed instruction pattern, not a measured custom-decoder speedup.

[^simd-11]: V8 team. [How we made JSON.stringify more than twice as fast](https://v8.dev/blog/json-stringify), August 4, 2025, string scanning with SIMD and SWAR.

[^simd-12]: Andrew Gallant and contributors. [`memchr` 2.8.3 documentation](https://docs.rs/memchr/2.8.3/memchr/), supported search predicates and architecture-specific acceleration.

[^simd-13]: Rust/Arm intrinsic documentation. [`vqtbl4q_u8`](https://doc.rust-lang.org/stable/core/arch/aarch64/fn.vqtbl4q_u8.html), lookup from four 16-byte vector registers.

[^simd-14]: simdutf8 contributors. [simdutf8 0.1.5 implementation selection](https://docs.rs/simdutf8/0.1.5/simdutf8/#implementation-selection), native dispatch and compile-time WASM SIMD requirements.
