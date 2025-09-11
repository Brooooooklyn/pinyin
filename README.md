# `@napi-rs/pinyin`

<p>
  <a href="https://https://github.com/Brooooooklyn/pinyin/actions"><img src="https://github.com/Brooooooklyn/pinyin/workflows/CI/badge.svg" alt="Build Status" /></a>
  <a href="https://npmcharts.com/compare/@napi-rs/pinyin?minimal=true"><img src="https://img.shields.io/npm/dm/@napi-rs/pinyin.svg?sanitize=true" alt="Downloads" /></a>
  <a href="https://github.com/Brooooooklyn/pinyin/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/@napi-rs/pinyin.svg?sanitize=true" alt="License" /></a>
</p>

> 🚀 Help me to become a full-time open-source developer by [sponsoring me on Github](https://github.com/sponsors/Brooooooklyn)

[rust-pinyin](https://github.com/mozillazg/rust-pinyin) Node.js 版本，WebAssembly 版本支持 Web/Node.js.

## 功能

- 超高性能
- 无 `postinstall` 与 `node-gyp`
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

Benchmark over [`pinyin`](https://github.com/hotoo/pinyin) and [`pinyin-pro`](https://github.com/zh-lx/pinyin-pro) package:

> **Note**
>
> [`pinyin-pro`](https://github.com/zh-lx/pinyin-pro) doesn't support segment feature.

System info

```
OS: macOS 12.3.1 21E258 arm64
Host: MacBookPro18,2
Kernel: 21.4.0
Shell: zsh 5.8
CPU: Apple M1 Max
GPU: Apple M1 Max
Memory: 9539MiB / 65536MiB
```

```bash
Running "Short input without segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    2 183 922 ops/s, ±0.32%   | fastest

  pinyin-pro:
    1 603 486 ops/s, ±0.10%   | slowest, 26.58% slower

  node-pinyin:
    2 150 629 ops/s, ±0.21%   | 1.52% slower

Finished 3 cases!
  Fastest: @napi-rs/pinyin
  Slowest: pinyin-pro
Running "Long input without segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    551 ops/s, ±0.55%   | fastest

  pinyin-pro:
    182 ops/s, ±11.67%   | slowest, 66.97% slower

  node-pinyin:
    226 ops/s, ±14.00%   | 58.98% slower

Finished 3 cases!
  Fastest: @napi-rs/pinyin
  Slowest: pinyin-pro
Running "Short input with segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    863 839 ops/s, ±0.61%   | fastest

  node-pinyin:
    710 893 ops/s, ±0.58%   | slowest, 17.71% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
Running "Long input with segment" suite...
Progress: 100%

  @napi-rs/pinyin:
    304 ops/s, ±1.99%   | fastest

  node-pinyin:
    8 ops/s, ±2.85%     | slowest, 97.37% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin
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
