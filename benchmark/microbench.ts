import { Bench } from 'tinybench'
import { pinyin as nodePinyin } from 'pinyin'

import { pinyin, PINYIN_STYLE } from '../index.js'

const short = '你好拼音'
const eight = '你好你好你好你好'

const bench = new Bench({ time: 1500 })

// Fixed-overhead references
bench.add("ours pinyin('') empty", () => pinyin(''))
bench.add("ours pinyin('你') 1 Han", () => pinyin('你'))
// Options-object parsing cost
bench.add("ours pinyin(short, {}) empty options", () => pinyin(short, {}))
bench.add('ours pinyin(short, {segment:false})', () => pinyin(short, { segment: false }))
// The benchmarked case + scaling
bench.add("ours pinyin('你好拼音') 4 Han", () => pinyin(short))
bench.add('ours pinyin 8 Han', () => pinyin(eight))
// node-pinyin scaling reference
bench.add("node-pinyin('你') 1 Han", () => nodePinyin('你'))
bench.add("node-pinyin('你好拼音')", () => nodePinyin(short))
bench.add('node-pinyin 8 Han', () => nodePinyin(eight))

await bench.run()
console.table(bench.table())

const bench2 = new Bench({ time: 1500 })
const buf = Buffer.from('你好拼音')
bench2.add("ours pinyin(string) 4 Han", () => pinyin('你好拼音'))
bench2.add('ours pinyin(Buffer) 4 Han', () => pinyin(buf))
await bench2.run()
console.table(bench2.table())
