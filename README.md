# `@napi-rs/pinyin`

<p>
  <a href="https://https://github.com/Brooooooklyn/pinyin/actions"><img src="https://github.com/Brooooooklyn/pinyin/workflows/CI/badge.svg" alt="Build Status" /></a>
  <a href="https://npmcharts.com/compare/@napi-rs/pinyin?minimal=true"><img src="https://img.shields.io/npm/dm/@napi-rs/pinyin.svg?sanitize=true" alt="Downloads" /></a>
  <a href="https://github.com/Brooooooklyn/pinyin/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/@napi-rs/pinyin.svg?sanitize=true" alt="License" /></a>
</p>

> 🚀 Help me to become a full-time open-source developer by [sponsoring me on Github](https://github.com/sponsors/Brooooooklyn)

[rust-pinyin](https://github.com/mozillazg/rust-pinyin) Node.js 版本，不支持 web.

## 功能

- 超高性能
- 无 `postinstall` 与 `node-gyp`，纯净安装无烦恼
- 跨 `Node.js` 版本支持，升级 Node 版本无需 `rebuild/reinstall`
- `linux alpine` 支持
- **分词**再转拼音
- 原生异步支持，可运行在 `libuv` 线程池中，不阻塞主线程

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

## 与 [pinyin](https://github.com/hotoo/pinyin) 性能对比

Benchmark over `pinyin` package:

```bash
Running "Short input without segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    962 035 ops/s, ±0.68%   | fastest

  node-pinyin:
    434 241 ops/s, ±0.66%   | slowest, 54.86% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
Running "Long input without segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    59 ops/s, ±0.83%   | fastest

  node-pinyin:
    2 ops/s, ±3.30%    | slowest, 96.61% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
Running "Short input with segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    530 228 ops/s, ±1.94%   | fastest

  node-pinyin:
    307 788 ops/s, ±0.83%   | slowest, 41.95% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
Running "Long input with segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    152 ops/s, ±1.09%   | fastest

  node-pinyin:
    3 ops/s, ±3.08%     | slowest, 98.03% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
✨  Done in 53.36s.
```

## 用法

### 同步

```ts
import { pinyin } from '@napi-rs/pinyin'

console.log(pinyin('中心')) // [ [ 'zhōng' ], [ 'xīn' ] ]
```

### 异步

```ts
import { asyncPinyin } from '@napi-rs/pinyin'

asyncPinyin('中心').then(console.log.bind(console)) // [ [ 'zhōng' ], [ 'xīn' ] ]
```

### 参数

- **input** `<string>`

  需要转拼音的中文字符串

- **options?** `<Options>`

  转拼音参数

  - **Options.heteronym?** `<boolean>`

    是否处理多音字， 默认 `false`。如果为 `true`，返回类型为 `string[][]/Promise<string[][]>`, 如果为 `false` 返回类型为 `string[]/Promise<string[]>`

  - **Options.style?** `<PINYIN_STYLE>`

    拼音风格，默认为 `PINYIN_STYLE.WithTone`
    可选值为:

    - `Plain` 普通风格，不带声调

    - `WithTone` 带声调的风格

    - `WithToneNum` 声调在各个拼音之后，使用数字 1-4 表示的风格

    - `WithToneNumEnd` 声调在拼音最后，使用数字 1-4 表示的风格

    - `FirstLetter` 首字母风格

  - **Options.segment?** `<boolean>`

    是否开启分词。输入有多音字时，开启分词可以获得更准确结果。
