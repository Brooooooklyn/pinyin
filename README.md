# `@napi-rs/pinyin`

![https://https://github.com/Brooooooklyn/pinyin/actions](https://github.com/Brooooooklyn/pinyin/workflows/CI/badge.svg)

> [rust-pinyin](https://github.com/mozillazg/rust-pinyin) binding for NodeJS.

## Install this package

```
yarn add @napi-rs/pinyin
```

## Support matrix

### Operating Systems

| Linux | macOS | Windows x64 MSVC |
| ----- | ----- | ---------------- |
| ✓     | ✓     | ✓                |

### NodeJS

| Node10 | Node 12 | Node14 | Node15 |
| ------ | ------- | ------ | ------ |
| ✓      | ✓       | ✓      | ✓      |

## Performance

Benchmark over `pinyin` package:

```bash
running "Short pinyin" suite...

Progress: 100%

  @napi-rs/pinyin:
    1 174 355 ops/s, ±0.95%   | fastest

  node-pinyin:
    419 694 ops/s, ±2.02%     | slowest, 64.26% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin

Running "Long pinyin" suite...
Progress: 100%

  @napi-rs/pinyin:
    347 ops/s, ±2.16%   | fastest

  node-pinyin:
    2 ops/s, ±5.43%     | slowest, 99.42% slower

Finished 2 cases!
  Fastest: @napi-rs/pinyin
  Slowest: node-pinyin

✨  Done in 27.72s.
```
