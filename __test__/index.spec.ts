import test from 'ava'

import { pinyin, asyncPinyin, PINYIN_STYLE } from '../index'

const styles = Object.values(PINYIN_STYLE) as PINYIN_STYLE[]

const STYLE_NAMES: Record<number, string> = Object.entries(PINYIN_STYLE).reduce((acc, [k, v]) => {
  // @ts-expect-error
  acc[v] = k
  return acc
}, {})

const fixtures = [
  // 单音字
  '我',
  // 多音字
  '中',
  // 单音词
  '我是谁',
  // 多音词
  '中国',
]

for (const fixture of fixtures) {
  for (const style of styles) {
    test(`(${fixture}) to pinyin without heteronym with [${STYLE_NAMES[style]}] style`, (t) => {
      t.snapshot(pinyin(fixture, false, style))
    })

    test(`(${fixture}) to pinyin with heteronym with [${STYLE_NAMES[style]}] style`, (t) => {
      t.snapshot(pinyin(fixture, true, style))
    })

    test(`(${fixture}) to pinyin async without heteronym with [${STYLE_NAMES[style]}] style`, async (t) => {
      t.snapshot(await asyncPinyin(fixture, false, style))
    })

    test(`(${fixture}) to pinyin async with heteronym with [${STYLE_NAMES[style]}] style`, async (t) => {
      t.snapshot(await asyncPinyin(fixture, true, style))
    })
  }
}
