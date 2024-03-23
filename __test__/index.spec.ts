import test from 'ava'

import { compare, pinyin, asyncPinyin, PINYIN_STYLE } from '../index'

// @ts-expect-error
const styles = Object.values(PINYIN_STYLE) as PINYIN_STYLE[]

// @ts-expect-error
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
  // 中英混合
  '拼音(pinyin)',
  // 中英混合，多音字
  '中国(china)',
  'aa',
  'a a',
]

for (const fixture of fixtures) {
  for (const style of styles) {
    test(`(${fixture}) to pinyin without heteronym with [${STYLE_NAMES[style]}] style`, (t) => {
      t.snapshot(
        pinyin(fixture, {
          style,
          heteronym: false,
        }),
      )
    })

    test(`(${fixture}) to pinyin with heteronym with [${STYLE_NAMES[style]}] style`, (t) => {
      t.snapshot(
        pinyin(fixture, {
          style,
          heteronym: true,
        }),
      )
    })

    test(`(${fixture}) to pinyin async without heteronym with [${STYLE_NAMES[style]}] style`, async (t) => {
      t.snapshot(
        await asyncPinyin(fixture, {
          style,
          heteronym: false,
        }),
      )
    })

    test(`(${fixture}) to pinyin async with heteronym with [${STYLE_NAMES[style]}] style`, async (t) => {
      t.snapshot(
        await asyncPinyin(fixture, {
          heteronym: true,
          style,
        }),
      )
    })
  }
}

test('mixed with segment', (t) => {
  t.deepEqual(pinyin('特殊天-1', { style: PINYIN_STYLE.Plain, segment: true }), ['te', 'shu', 'tian', '-1'])
})

test('mixed with multi segment', (t) => {
  t.deepEqual(pinyin('特殊天-1', { style: PINYIN_STYLE.Plain, segment: true, heteronym: true }), [
    ['te'],
    ['shu'],
    ['tian'],
    ['-1'],
  ])
})

test('我,要,排,序 => 序,我,排,要', (t) => {
  const data = '我要排序'.split('')
  const sortedData = data.sort(compare)
  t.deepEqual(sortedData, '排我序要'.split(''))
})

test('b啊 => 啊b', (t) => {
  const data = 'b啊'.split('')
  const sortedData = data.sort(compare)
  t.deepEqual(sortedData, '啊b'.split(''))
})

test('饿鹅312cba => 123abc鹅饿', (t) => {
  const data = '饿鹅312cba'.split('')
  const sortedData = data.sort(compare)
  t.deepEqual(sortedData, '123abc鹅饿'.split(''))
})

test('能比较多个文字的字符串', (t) => {
  const smaller = '蜘蛛侠1'
  const middle = '蜘蛛侠12'
  const greater = '蜘蛛侠3'
  const empty = ''

  t.deepEqual(compare(smaller, middle), -1)
  t.deepEqual(compare(middle, middle), 0)
  t.deepEqual(compare(middle, greater), -1)
  t.deepEqual(compare(greater, middle), 1)
  t.deepEqual(compare(empty, empty), 0)
})

test('能比较 emoji', (t) => {
  const smaller = '😀'
  const middle = '😃'
  const greater = '😄'

  t.deepEqual(compare(smaller, middle), -1)
  t.deepEqual(compare(middle, middle), 0)
  t.deepEqual(compare(middle, greater), -1)
  t.deepEqual(compare(greater, middle), 1)
})
