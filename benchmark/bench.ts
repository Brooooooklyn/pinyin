import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { Bench } from 'tinybench'
import { pinyin as nodePinyin } from 'pinyin'
import { pinyin as pinyinPro } from 'pinyin-pro'

import { pinyin } from '../index'

const short = '你好拼音'
const long = readFileSync(join(import.meta.dirname, 'long.txt'))
const longText = long.toString('utf8')

// Short input without segment
const shortBench = new Bench()

shortBench
  .add('@napi-rs/pinyin', () => {
    pinyin(short)
  })
  .add('pinyin-pro', () => {
    pinyinPro(short)
  })
  .add('node-pinyin', () => {
    nodePinyin(short)
  })

console.log('Running "Short input without segment" suite...')
await shortBench.run()

console.table(shortBench.table())

// Long input without segment
const longBench = new Bench()

longBench
  .add('@napi-rs/pinyin', () => {
    pinyin(long)
  })
  .add('pinyin-pro', () => {
    pinyinPro(longText)
  })
  .add('node-pinyin', () => {
    nodePinyin(longText)
  })

console.log('Running "Long input without segment" suite...')
await longBench.run()
console.table(longBench.table())

// Short input with segment
const shortSegmentBench = new Bench()

shortSegmentBench
  .add('@napi-rs/pinyin', () => {
    pinyin(short, { segment: true })
  })
  .add('node-pinyin', () => {
    nodePinyin(short, { segment: true })
  })

console.log('Running "Short input with segment" suite...')
await shortSegmentBench.run()
console.table(shortSegmentBench.table())

// Long input with segment
const longSegmentBench = new Bench()

longSegmentBench
  .add('@napi-rs/pinyin', () => {
    pinyin(long, { segment: true })
  })
  .add('node-pinyin', () => {
    nodePinyin(longText, { segment: true })
  })

console.log('Running "Long input with segment" suite...')
await longSegmentBench.run()
console.table(longSegmentBench.table())
