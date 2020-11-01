import { promises as fs, readFileSync } from 'fs'
import { join } from 'path'

import b from 'benny'
import { Summary } from 'benny/lib/internal/common-types'
import nodePinyin from 'pinyin'

import { pinyin } from '../index'

const short = '你好拼音'
const long = readFileSync(join(__dirname, 'long.txt'), 'utf-8')

async function run() {
  const output = [
    await b.suite(
      'Short input without segment',

      b.add('@napi-rs/pinyin', () => {
        pinyin(short)
      }),

      b.add('node-pinyin', () => {
        nodePinyin(short)
      }),

      b.cycle(),
      b.complete(),
    ),

    await b.suite(
      'Long input without segment',

      b.add('@napi-rs/pinyin', () => {
        pinyin(long)
      }),

      b.add('node-pinyin', () => {
        nodePinyin(long)
      }),

      b.cycle(),
      b.complete(),
    ),
    await b.suite(
      'Short input with segment',

      b.add('@napi-rs/pinyin', () => {
        pinyin(short, { segment: true })
      }),

      b.add('node-pinyin', () => {
        nodePinyin(short, { segment: true })
      }),

      b.cycle(),
      b.complete(),
    ),

    await b.suite(
      'Long input with segment',

      b.add('@napi-rs/pinyin', () => {
        pinyin(long, { segment: true })
      }),

      b.add('node-pinyin', () => {
        nodePinyin(long, { segment: true })
      }),

      b.cycle(),
      b.complete(),
    ),
  ]
    .map(formatSummary)
    .join('\n')

  await fs.writeFile(join(process.cwd(), 'bench.txt'), output, 'utf8')
}

function formatSummary(summary: Summary): string {
  return summary.results
    .map(
      (result) =>
        `${summary.name}#${result.name} x ${result.ops} ops/sec ±${result.margin}% (${result.samples} runs sampled)`,
    )
    .join('\n')
}

run().catch((e) => {
  console.error(e)
})
