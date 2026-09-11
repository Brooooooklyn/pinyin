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

|                  | node12 | node14 | node16 | node17 |
| ---------------- | ------ | ------ | ------ | ------ |
| Windows x64      | ✓      | ✓      | ✓      | ✓      |
| Windows x32      | ✓      | ✓      | ✓      | ✓      |
| Windows arm64    | ✓      | ✓      | ✓      | ✓      |
| macOS x64        | ✓      | ✓      | ✓      | ✓      |
| macOS arm64      | ✓      | ✓      | ✓      | ✓      |
| Linux x64 gnu    | ✓      | ✓      | ✓      | ✓      |
| Linux x64 musl   | ✓      | ✓      | ✓      | ✓      |
| Linux arm gnu    | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 gnu  | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 musl | ✓      | ✓      | ✓      | ✓      |
| Android arm64    | ✓      | ✓      | ✓      | ✓      |
| Android armv7    | ✓      | ✓      | ✓      | ✓      |
| FreeBSD x64      | ✓      | ✓      | ✓      | ✓      |

## 性能与算法

实现包含静态 Unicode 查表、预计算的五种拼音格式、4,083 条词语读音、零分配字符迭代，以及针对 JavaScript 数组和字符串的批量输出。Rust core 默认无运行时依赖，可通过 `jieba` feature 启用分词集成；Node.js 和 WebAssembly 构建已包含此功能。

[统一的算法与性能研究报告](docs/performance-research.md)包含 Rust core、Jieba 集成、simdutf8 校验、UTF-16 输入输出、JSON 转义与 SIMD 优化，保留各阶段基准、原始数据、测试方法和复现步骤。对比明确区分相同输出的合成数据与词典、读音策略不同的自然文本。

`yarn build:wasm:simd` 将额外启用 SIMD128 优化的 WebAssembly 版本单独输出到 `target/wasi-simd`；标准版本也需要支持 SIMD128 的引擎。

```sh
yarn build
yarn bench
cargo bench -p napi-pinyin-core --bench throughput
```

基准的输入哈希、运行环境和原始样本保存在 [`benchmark/results`](benchmark/results)。这些是特定硬件和运行时上的测量结果，不代表所有输入上的绝对性能上限。

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

    是否使用词语词典选择上下文读音，默认 `false`。例如 `pinyin('重庆银行音乐', { segment: true })` 返回 `['chong', 'qing', 'yin', 'hang', 'yin', 'yue']`。此行为修正了旧版只分词却未改变读音的问题；`heteronym: true` 仍返回各字符的全部候选读音。

  - **Options.segmenter?** `<'phrase' | 'jieba'>`

    `segment: true` 时使用的读音选择方式，默认 `'phrase'`。`'jieba'` 使用 `jieba-rs` 分词，优先选择词内的读音匹配，同时保留跨词边界的词典回退（例如「划分 / 为」中的「分为」）。未匹配字符使用默认读音。Node 接口使用 `HMM=false`；首次非 ASCII 的 Jieba 转换会初始化共享词典。`segment: false` 或 `heteronym: true` 不执行分词。

```ts
pinyin('重庆银行音乐', { segment: true, segmenter: 'jieba' })
// ['chong', 'qing', 'yin', 'hang', 'yin', 'yue']
await asyncPinyin('重庆银行音乐', { segment: true, segmenter: 'jieba' })
```

### 直接返回字符串

```ts
import { pinyinString, PINYIN_STYLE } from '@napi-rs/pinyin'

pinyinString('重庆银行', { segment: true, style: PINYIN_STYLE.WithTone })
// 'chóng qìng yín háng'
pinyinString('中国', { separator: '-' })
// 'zhong-guo'
```

`pinyinString` 接受 `string | Uint8Array`，支持 `style`、`segment`、`segmenter` 和 `separator`（默认空格）。当最终需要文本时，它能避免先创建大量 JavaScript 数组元素再拼接。连续的非汉字内容按原样保留为一个片段，非法 UTF-8 字节输入会报错。

Rust 使用方式与词典来源见 [`napi-pinyin-core`](crates/pinyin-core)。词典保留了上游 MIT 许可；本实现没有提供 pinyin-pro 的所有自定义词典、姓氏和变调选项。
