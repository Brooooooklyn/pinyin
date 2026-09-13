# `@napi-rs/pinyin`

<p>
  <a href="https://https://github.com/Brooooooklyn/pinyin/actions"><img src="https://github.com/Brooooooklyn/pinyin/workflows/CI/badge.svg" alt="Build Status" /></a>
  <a href="https://npmcharts.com/compare/@napi-rs/pinyin?minimal=true"><img src="https://img.shields.io/npm/dm/@napi-rs/pinyin.svg?sanitize=true" alt="Downloads" /></a>
  <a href="https://github.com/Brooooooklyn/pinyin/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/@napi-rs/pinyin.svg?sanitize=true" alt="License" /></a>
</p>

> 🚀 Help me to become a full-time open-source developer by [sponsoring me on Github](https://github.com/sponsors/Brooooooklyn)

基于本仓库独立 Rust crate [`napi-pinyin-core`](crates/pinyin-core) 的中文转拼音实现，同时支持 Node.js 原生模块和 WebAssembly。

## 功能

- 超高性能
- 无 `postinstall` 与 `node-gyp`
- 跨 `Node.js` 版本支持，升级 Node 版本无需 `rebuild/reinstall`
- `linux alpine` 支持
- 基于静态词典的上下文读音选择，支持重庆、银行、音乐等多音词
- 可选 `jieba-rs` 分词，使用词边界引导词语读音选择
- 原生异步支持，查表、词语读音选择和批量编码在 `libuv` 线程池中执行，JavaScript 结果创建仍在主线程

## 安装

```
yarn add @napi-rs/pinyin
```

## 系统/Node.js 版本

<!-- Keep the image query and text matrix in sync with package.json / CI.
     Use Markdown image syntax so previews do not need raw HTML support. -->

![@napi-rs/pinyin 兼容性：engines.node 声明 >= 10.0，当前 CI 测试 Node 24。17 个原生目标：8 个通过运行时测试，9 个仅构建。WASI 的独立测试范围见下方文字表格。](https://napi.rs/support-matrix.png?name=%40napi-rs%2Fpinyin&engines=%3E%3D+10.0&nodeTested=24&tested=x86_64-pc-windows-msvc%2Caarch64-pc-windows-msvc%2Cx86_64-apple-darwin%2Caarch64-apple-darwin%2Cx86_64-unknown-linux-gnu%2Cx86_64-unknown-linux-musl%2Caarch64-unknown-linux-gnu%2Caarch64-unknown-linux-musl&untested=armv7-unknown-linux-gnueabihf%2Ci686-pc-windows-msvc%2Caarch64-linux-android%2Cx86_64-unknown-freebsd%2Carmv7-linux-androideabi%2Cpowerpc64le-unknown-linux-gnu%2Cs390x-unknown-linux-gnu%2Criscv64gc-unknown-linux-gnu%2Caarch64-unknown-linux-ohos)

图中 Node.js 范围来自 `package.json` 的 `engines.node: ">= 10.0"`；当前原生 CI 测试 Node **24**；Node 24 不再提供 armv7 预编译包，Linux armv7 仅构建。声明范围不表示其他 Node.js 版本均已通过当前测试。

<details>
<summary>完整兼容性表格（文字版）</summary>

### Node.js

| 版本  | package.json 声明允许 | 当前运行时 CI  |
| ----- | --------------------- | -------------- |
| < 10  | 否                    | 无             |
| 10–21 | 是                    | 无             |
| 22    | 是                    | 无             |
| 23    | 是                    | 无             |
| 24    | 是                    | 有，目标见下表 |
| ≥ 25  | 是                    | 无             |

### 构建与测试目标

范围以 [构建配置](package.json)和 [CI 工作流](.github/workflows/CI.yml)为准。“仅构建”表示生成了构建产物，CI 未执行该目标的运行时测试。

| Rust target                     | 平台               | 当前运行时 CI                 |
| ------------------------------- | ------------------ | ----------------------------- |
| `x86_64-pc-windows-msvc`        | Windows x64        | Node 24                       |
| `aarch64-pc-windows-msvc`       | Windows arm64      | Node 24                       |
| `i686-pc-windows-msvc`          | Windows x32        | 仅构建                        |
| `x86_64-apple-darwin`           | macOS x64          | Node 24                       |
| `aarch64-apple-darwin`          | macOS arm64        | Node 24                       |
| `x86_64-unknown-linux-gnu`      | Linux x64 GNU      | Node 24                       |
| `x86_64-unknown-linux-musl`     | Linux x64 musl     | Node 24                       |
| `aarch64-unknown-linux-gnu`     | Linux arm64 GNU    | Node 24                       |
| `aarch64-unknown-linux-musl`    | Linux arm64 musl   | Node 24                       |
| `armv7-unknown-linux-gnueabihf` | Linux armv7 GNU    | 仅构建                        |
| `powerpc64le-unknown-linux-gnu` | Linux ppc64le GNU  | 仅构建                        |
| `s390x-unknown-linux-gnu`       | Linux s390x GNU    | 仅构建                        |
| `riscv64gc-unknown-linux-gnu`   | Linux riscv64 GNU  | 仅构建                        |
| `aarch64-linux-android`         | Android arm64      | 仅构建                        |
| `armv7-linux-androideabi`       | Android armv7      | 仅构建                        |
| `x86_64-unknown-freebsd`        | FreeBSD x64        | 仅构建                        |
| `aarch64-unknown-linux-ohos`    | OpenHarmony arm64  | 仅构建                        |
| `wasm32-wasip1-threads`         | WebAssembly / WASI | Node 24；标准和额外 SIMD 构建 |

共 **18 个构建目标**：17 个原生目标（8 个有运行时测试、9 个仅构建），以及 1 个 WASI 目标。当前矩阵没有非阻塞运行时测试目标。

### WebAssembly 与浏览器

`wasm32-wasip1-threads` 通过 NAPI-RS 构建。CI 在 Node 24 中分别加载标准 WASI 模块和额外启用 SIMD128 的模块，运行完整绑定测试；标准模块也需要 SIMD128 支持。

仓库提供浏览器加载器，但尚未执行浏览器 CI。浏览器使用需要 SIMD128、共享 WebAssembly 内存和 Worker 支持；页面须启用[跨源隔离](https://developer.mozilla.org/en-US/docs/Web/API/Window/crossOriginIsolated)（通常使用 `Cross-Origin-Opener-Policy: same-origin` 和 `Cross-Origin-Embedder-Policy: require-corp`）。WASI 在 Node 中的测试结果不代表浏览器验证结果。

</details>

## 性能与算法

实现包含静态 Unicode 查表、预计算的五种拼音格式、4,083 条词语读音、零分配字符迭代，以及针对 JavaScript 数组和字符串的批量输出。Rust core 默认无运行时依赖，可通过 `jieba` feature 启用分词集成；Node.js 和 WebAssembly 构建已包含此功能。

[统一的算法与性能研究报告](docs/performance-research.md)包含 Rust core、Jieba 集成、simdutf8 校验、UTF-16 输入输出、JSON 转义与 SIMD 优化，保留各阶段基准、原始数据、测试方法和复现步骤。对比明确区分相同输出的合成数据与词典、读音策略不同的自然文本。

`yarn build:wasm:simd` 将额外启用 SIMD128 优化的 WebAssembly 版本单独输出到 `target/wasi-simd`；标准版本也需要支持 SIMD128 的引擎。

```sh
yarn build
```

研究报告保留测量结果，并链接到历史提交中的基准源码与原始样本。这些是特定硬件和运行时上的测量结果，不代表所有输入上的绝对性能上限。

## 与 [pinyin](https://github.com/hotoo/pinyin) 性能对比

Benchmark over [`pinyin`](https://github.com/hotoo/pinyin) and [`pinyin-pro`](https://github.com/zh-lx/pinyin-pro) package:

> **Note**
>
> [`pinyin-pro`](https://github.com/zh-lx/pinyin-pro) doesn't support segment feature.

System info

```
OS: macOS 26.6.2 25G83 arm64
Host: Mac17,6
Kernel: 25.6.0
Shell: zsh 5.9
CPU: Apple M5 Max
GPU: Apple M5 Max
Memory: 63801MiB / 131072MiB
```

```bash
Running "Short input without segment" suite...
┌─────────┬───────────────────┬──────────────────┬──────────────────┬────────────────────────┬────────────────────────┬─────────┐
│ (index) │ Task name         │ Latency avg (ns) │ Latency med (ns) │ Throughput avg (ops/s) │ Throughput med (ops/s) │ Samples │
├─────────┼───────────────────┼──────────────────┼──────────────────┼────────────────────────┼────────────────────────┼─────────┤
│ 0       │ '@napi-rs/pinyin' │ '252.81 ± 0.09%' │ '250.00 ± 0.00'  │ '4036817 ± 0.01%'      │ '4000000 ± 0'          │ 3955576 │
│ 1       │ 'pinyin-pro'      │ '483.06 ± 5.03%' │ '458.00 ± 1.00'  │ '2199830 ± 0.01%'      │ '2183406 ± 4757'       │ 2070133 │
│ 2       │ 'node-pinyin'     │ '185.26 ± 3.34%' │ '167.00 ± 0.00'  │ '5814055 ± 0.01%'      │ '5988024 ± 0'          │ 5397793 │
└─────────┴───────────────────┴──────────────────┴──────────────────┴────────────────────────┴────────────────────────┴─────────┘
Running "Long input without segment" suite...
┌─────────┬───────────────────┬───────────────────┬───────────────────┬────────────────────────┬────────────────────────┬─────────┐
│ (index) │ Task name         │ Latency avg (ns)  │ Latency med (ns)  │ Throughput avg (ops/s) │ Throughput med (ops/s) │ Samples │
├─────────┼───────────────────┼───────────────────┼───────────────────┼────────────────────────┼────────────────────────┼─────────┤
│ 0       │ '@napi-rs/pinyin' │ '484810 ± 1.92%'  │ '464750 ± 8958.0' │ '2130 ± 0.34%'         │ '2152 ± 41'            │ 2069    │
│ 1       │ 'pinyin-pro'      │ '2538670 ± 1.24%' │ '2438917 ± 65646' │ '399 ± 0.95%'          │ '410 ± 11'             │ 394     │
│ 2       │ 'node-pinyin'     │ '1115280 ± 0.98%' │ '1053000 ± 24834' │ '911 ± 0.71%'          │ '950 ± 23'             │ 897     │
└─────────┴───────────────────┴───────────────────┴───────────────────┴────────────────────────┴────────────────────────┴─────────┘
Running "Short input with segment" suite...
┌─────────┬───────────────────┬──────────────────┬──────────────────┬────────────────────────┬────────────────────────┬─────────┐
│ (index) │ Task name         │ Latency avg (ns) │ Latency med (ns) │ Throughput avg (ops/s) │ Throughput med (ops/s) │ Samples │
├─────────┼───────────────────┼──────────────────┼──────────────────┼────────────────────────┼────────────────────────┼─────────┤
│ 0       │ '@napi-rs/pinyin' │ '565.64 ± 2.77%' │ '542.00 ± 1.00'  │ '1828665 ± 0.01%'      │ '1845018 ± 3410'       │ 1767902 │
│ 1       │ 'node-pinyin'     │ '1274.4 ± 3.91%' │ '1208.0 ± 83.00' │ '832284 ± 0.03%'       │ '827815 ± 53221'       │ 784685  │
└─────────┴───────────────────┴──────────────────┴──────────────────┴────────────────────────┴────────────────────────┴─────────┘
Running "Long input with segment" suite...
┌─────────┬───────────────────┬────────────────────┬──────────────────────┬────────────────────────┬────────────────────────┬─────────┐
│ (index) │ Task name         │ Latency avg (ns)   │ Latency med (ns)     │ Throughput avg (ops/s) │ Throughput med (ops/s) │ Samples │
├─────────┼───────────────────┼────────────────────┼──────────────────────┼────────────────────────┼────────────────────────┼─────────┤
│ 0       │ '@napi-rs/pinyin' │ '806677 ± 1.60%'   │ '781937 ± 11562'     │ '1264 ± 0.40%'         │ '1279 ± 19'            │ 1240    │
│ 1       │ 'node-pinyin'     │ '85591557 ± 1.14%' │ '84388751 ± 1751896' │ '12 ± 1.05%'           │ '12 ± 0'               │ 64      │
└─────────┴───────────────────┴────────────────────┴──────────────────────┴────────────────────────┴────────────────────────┴─────────┘
```

## 用法

### 同步

```ts
import { pinyin } from '@napi-rs/pinyin'

console.log(pinyin('中心')) // ['zhong', 'xin']
```

### 异步

```ts
import { asyncPinyin } from '@napi-rs/pinyin'

asyncPinyin('中心').then(console.log.bind(console)) // ['zhong', 'xin']
```

### 参数

- **input** `<string | Uint8Array>`（同步）或 `<string | Buffer>`（异步）

  需要转拼音的字符串或有效 UTF-8 字节；异步调用会在提交任务前复制输入 Buffer。

- **options?** `<Options>`

  转拼音参数
  - **Options.heteronym?** `<boolean>`

    是否处理多音字， 默认 `false`。如果为 `true`，返回类型为 `string[][]/Promise<string[][]>`, 如果为 `false` 返回类型为 `string[]/Promise<string[]>`

  - **Options.style?** `<PINYIN_STYLE>`

    拼音风格，默认为 `PINYIN_STYLE.Plain`
    可选值为:
    - `Plain` 普通风格，不带声调

    - `WithTone` 带声调的风格

    - `WithToneNum` 声调在各个拼音之后，使用数字 1-4 表示的风格

    - `WithToneNumEnd` 声调在拼音最后，使用数字 1-4 表示的风格

    - `FirstLetter` 首字母风格

  - **Options.segment?** `<boolean>`

    是否分词，默认 `false`。未指定 `segmenter` 时完全保留旧版 Jieba 分词行为与逐字读音：`pinyin('重庆银行音乐', { segment: true })` 返回 `['zhong', 'qing', 'yin', 'xing', 'yin', 'le']`。旧版混合词处理也保留，例如 `B超` 返回 `['chao']`，包括 `heteronym: true` 时忽略词内未收录字符的行为。

  - **Options.segmenter?** `<'phrase' | 'jieba'>`

    显式启用 `segment: true` 的上下文读音选择；省略时保留旧版行为。`'phrase'` 直接匹配读音词典；`'jieba'` 使用 `jieba-rs` 分词，优先选择词内的读音匹配，同时保留跨词边界的词典回退（例如「划分 / 为」中的「分为」）。未匹配字符使用默认读音。Node 接口使用 `HMM=false`；首次非 ASCII 的 Jieba 转换会初始化共享词典。显式指定 `segmenter` 时，`segment: false` 或 `heteronym: true` 使用逐字读音并跳过上下文分词；省略 `segmenter` 的旧版多音字分词行为不变。

```ts
pinyin('重庆银行音乐', { segment: true, segmenter: 'jieba' })
// ['chong', 'qing', 'yin', 'hang', 'yin', 'yue']
await asyncPinyin('重庆银行音乐', { segment: true, segmenter: 'jieba' })
```

### 直接返回字符串

```ts
import { pinyinString, PINYIN_STYLE } from '@napi-rs/pinyin'

pinyinString('重庆银行', { segment: true, segmenter: 'phrase', style: PINYIN_STYLE.WithTone })
// 'chóng qìng yín háng'
pinyinString('中国', { separator: '-' })
// 'zhong-guo'
```

`pinyinString` 接受 `string | Uint8Array`，支持 `style`、`segment`、`segmenter` 和 `separator`（默认空格）。当最终需要文本时，它能避免先创建大量 JavaScript 数组元素再拼接。结果与相同选项的 `pinyin(...).join(separator)` 一致，非法 UTF-8 字节输入会报错。

Rust 使用方式与词典来源见 [`napi-pinyin-core`](crates/pinyin-core)。词典保留了上游 MIT 许可；本实现没有提供 pinyin-pro 的所有自定义词典、姓氏和变调选项。
