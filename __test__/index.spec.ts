import test from 'ava'
import { createRequire } from 'node:module'
import { Worker } from 'node:worker_threads'

const { compare, pinyin, asyncPinyin, pinyinString, PINYIN_STYLE } = createRequire(import.meta.url)(
  './helpers/binding.cjs',
) as typeof import('../index')

// The native enum members are non-enumerable, so Object.values silently made
// this entire matrix empty. Enumerate the public values explicitly.
const styles = [
  PINYIN_STYLE.Plain,
  PINYIN_STYLE.WithTone,
  PINYIN_STYLE.WithToneNum,
  PINYIN_STYLE.WithToneNumEnd,
  PINYIN_STYLE.FirstLetter,
]

const STYLE_NAMES = ['Plain', 'WithTone', 'WithToneNum', 'WithToneNumEnd', 'FirstLetter']

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

test('issue #244: asyncPinyin with segment should not duplicate', async (t) => {
  t.deepEqual(
    await asyncPinyin('中心', { style: PINYIN_STYLE.Plain, segment: true }),
    ['zhong', 'xin'], // was ['zhong', 'zhong', 'xin', 'xin'] before the fix
  )
})

test('async mixed with segment', async (t) => {
  t.deepEqual(await asyncPinyin('特殊天-1', { style: PINYIN_STYLE.Plain, segment: true }), ['te', 'shu', 'tian', '-1'])
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

test('reject invalid UTF-8 Uint8Array', (t) => {
  // 0xFF 0xFE is not valid UTF-8
  const invalid = new Uint8Array([0xff, 0xfe, 0xff])
  t.throws(() => pinyin(invalid), { message: /valid UTF-8/ })
})

test('reject invalid UTF-8 Buffer in asyncPinyin', async (t) => {
  // 0xFF 0xFE is not valid UTF-8
  const invalid = Buffer.from([0xff, 0xfe, 0xff])
  await t.throwsAsync(() => asyncPinyin(invalid), { message: /valid UTF-8/ })
})

test('accept valid UTF-8 Uint8Array', (t) => {
  t.deepEqual(pinyin(new TextEncoder().encode('中国')), pinyin('中国'))
})

test('accept valid UTF-8 Buffer in asyncPinyin', async (t) => {
  t.deepEqual(await asyncPinyin(Buffer.from('中国')), await asyncPinyin('中国'))
})

for (const style of styles) {
  for (const segment of [false, true]) {
    for (const heteronym of [false, true]) {
      test(`sync/async/buffer Unicode parity ${style}/${segment}/${heteronym}`, async (t) => {
        const options = { style, segment, heteronym }
        for (const text of [
          '',
          'ASCII\0\t\n',
          '重庆银行音乐',
          'A👨‍👩‍👧‍👦中\u0301文\0 Z',
          '𠮷野家𠀀',
          'x'.repeat(8192) + '中🙂国',
        ]) {
          const expected = pinyin(text, options)
          t.deepEqual(pinyin(Buffer.from(text), options), expected)
          t.deepEqual(await asyncPinyin(text, options), expected)
          t.deepEqual(await asyncPinyin(Buffer.from(text), options), expected)
          if (!heteronym) {
            t.is(pinyinString(text, { style, segment, separator: '|' }), (expected as string[]).join('|'))
          }
        }
      })
    }
  }
}

test('segment resolves phrases and heteronym retains all readings', (t) => {
  t.deepEqual(pinyin('重庆银行音乐'), ['zhong', 'qing', 'yin', 'xing', 'yin', 'le'])
  t.deepEqual(pinyin('重庆银行音乐', { segment: true }), ['chong', 'qing', 'yin', 'hang', 'yin', 'yue'])
  t.deepEqual(pinyin('重庆银行音乐', { segment: true, heteronym: true }), pinyin('重庆银行音乐', { heteronym: true }))
})

for (const style of styles) {
  test(`Jieba sync/async/string/Unicode parity ${style}`, async (t) => {
    const options = { style, segment: true, segmenter: 'jieba' as const }
    for (const input of [
      '',
      'ASCII\0\r\n',
      '重庆银行音乐',
      '𠮷👨‍👩‍👧‍👦重\u0301庆\0🙂银行',
      '中"\\\t\n\u2028文'.repeat(100),
    ]) {
      const expected = pinyin(input, options) as string[]
      t.deepEqual(pinyin(Buffer.from(input), options), expected)
      t.deepEqual(await asyncPinyin(input, options), expected)
      t.deepEqual(await asyncPinyin(Buffer.from(input), options), expected)
      t.is(pinyinString(input, { ...options, separator: '|🙂\0' }), expected.join('|🙂\0'))
      t.deepEqual(pinyin(input, { ...options, heteronym: true }), pinyin(input, { style, heteronym: true }))
      t.deepEqual(await asyncPinyin(input, { ...options, heteronym: true }), pinyin(input, { style, heteronym: true }))
      t.deepEqual(pinyin(input, { ...options, segment: false }), pinyin(input, { style }))
    }
  })
}

test('Jieba resolves word pronunciations and snapshots async input', async (t) => {
  const options = { segment: true, segmenter: 'jieba' as const }
  t.deepEqual(pinyin('重庆银行音乐', options), ['chong', 'qing', 'yin', 'hang', 'yin', 'yue'])
  for (const text of ['划分为', '统称为']) {
    const toneOptions = { ...options, style: PINYIN_STYLE.WithTone }
    t.deepEqual(pinyin(text, toneOptions), pinyin(text, { segment: true, style: PINYIN_STYLE.WithTone }))
    t.is((pinyin(text, toneOptions) as string[]).at(-1), 'wéi')
  }
  const input = Buffer.from('重庆银行音乐'.repeat(2000))
  const expected = pinyin(input, options)
  const pending = asyncPinyin(input, options)
  input.fill(0xff)
  t.deepEqual(await pending, expected)
})

test('segmenter names and Jieba byte inputs are validated', async (t) => {
  const invalidOptions = { segment: true, segmenter: 'typo' as 'jieba' }
  t.throws(() => pinyin('中国', invalidOptions), { message: /Unknown segmenter/ })
  t.throws(() => pinyinString('中国', invalidOptions), { message: /Unknown segmenter/ })
  t.throws(() => asyncPinyin('中国', invalidOptions), { message: /Unknown segmenter/ })
  const invalid = Buffer.from([0xed, 0xa0, 0x80])
  const options = { segment: true, segmenter: 'jieba' as const }
  t.throws(() => pinyin(invalid, options), { message: /valid UTF-8/ })
  t.throws(() => pinyinString(invalid, options), { message: /valid UTF-8/ })
  await t.throwsAsync(() => asyncPinyin(invalid, options), { message: /valid UTF-8/ })
})

test('async conversion owns its buffer snapshot', async (t) => {
  const input = Buffer.from('重庆银行音乐'.repeat(2000))
  const expected = pinyin(input, { segment: true, style: PINYIN_STYLE.WithTone })
  const pending = asyncPinyin(input, { segment: true, style: PINYIN_STYLE.WithTone })
  input.fill(0xff)
  t.deepEqual(await pending, expected)
})

test('reject malformed and truncated UTF-8 in every API', async (t) => {
  for (const bytes of [[0xc0, 0xaf], [0xe4, 0xb8], [0xed, 0xa0, 0x80], [0xf4, 0x90, 0x80, 0x80], [0x80]]) {
    const input = Buffer.from(bytes)
    t.throws(() => pinyin(input), { message: /valid UTF-8/ })
    t.throws(() => pinyinString(input), { message: /valid UTF-8/ })
    await t.throwsAsync(() => asyncPinyin(input), { message: /valid UTF-8/ })
  }
})

test('typed array byte offset and output array isolation', (t) => {
  const buffer = Buffer.from('A中国Z')
  t.deepEqual(pinyin(buffer.subarray(1, -1)), ['zhong', 'guo'])
  const result = pinyin('中'.repeat(1000), { heteronym: true }) as string[][]
  result[0][0] = 'changed'
  t.not(result[1][0], 'changed')
  t.not((pinyin('中', { heteronym: true }) as string[][])[0][0], 'changed')
})

test('UTF-8 validation accepts unaligned SIMD boundaries in every API', async (t) => {
  for (const offset of [0, 1, 7, 15, 31]) {
    for (const boundary of [63, 64, 65, 127, 128, 129, 4095]) {
      const text = 'a'.repeat(boundary) + 'é中国🙂\0' + '重庆银行音乐'.repeat(40)
      // Invalid surrounding bytes must not be included in the borrowed slice.
      const backing = Buffer.concat([Buffer.alloc(offset, 0xff), Buffer.from(text), Buffer.from([0xff])])
      const buffer = backing.subarray(offset, -1)
      const view = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
      const options = { style: PINYIN_STYLE.WithTone, segment: true }
      t.deepEqual(pinyin(view, options), pinyin(text, options))
      t.is(pinyinString(buffer, options), pinyinString(text, options))
      t.deepEqual(await asyncPinyin(buffer, options), pinyin(text, options))
    }
  }
})

test('UTF-8 validation preserves exact error offsets and lengths at SIMD boundaries', async (t) => {
  const malformed = [
    { bytes: [0xff], length: 1 },
    { bytes: [0x80], length: 1 },
    { bytes: [0xc0, 0xaf], length: 1 },
    { bytes: [0xed, 0xa0, 0x80], length: 1 },
    { bytes: [0xf4, 0x90, 0x80, 0x80], length: 1 },
    { bytes: [0xe4, 0xb8, 0x61], length: 2 },
    { bytes: [0xf0, 0x9f, 0x99, 0x61], length: 3 },
    { bytes: [0xe4, 0xb8], length: null },
    { bytes: [0xf0, 0x9f, 0x99], length: null },
  ]
  for (const offset of [0, 1, 63, 64, 65, 127, 128, 129, 4095, 65535]) {
    for (const { bytes, length } of malformed) {
      const backing = Buffer.concat([
        Buffer.from([0xff]),
        Buffer.alloc(offset, 0x61),
        Buffer.from(bytes),
        // Incomplete sequences must remain at EOF; invalid sequences also test an early error.
        Buffer.alloc(length === null ? 0 : 1024, 0x61),
      ])
      const input = backing.subarray(1)
      const message =
        'Input buffer must contain valid UTF-8: ' +
        (length === null
          ? `incomplete utf-8 byte sequence from index ${offset}`
          : `invalid utf-8 sequence of ${length} bytes from index ${offset}`)
      const expected = { code: 'InvalidArg', message }
      t.throws(() => pinyin(input), expected)
      t.throws(() => pinyinString(input), expected)
      await t.throwsAsync(() => asyncPinyin(input), expected)
    }
  }
})

test('string defaults and arbitrary separators', (t) => {
  t.is(pinyinString('中国'), 'zhong guo')
  t.is(pinyinString(''), '')
  t.is(pinyinString('中\0国', { separator: '\0🙂' }), 'zhong\0🙂\0\0🙂guo')
  const unchanged = '\ufeff🙂\ufffe\uffff\0'
  for (const style of styles) {
    t.deepEqual(pinyin(unchanged, { style }), [unchanged])
    t.is(pinyinString(unchanged, { style }), unchanged)
  }
})

test('UTF-16 input preserves JS surrogate replacement across every output path', async (t) => {
  const controls = Array.from({ length: 32 }, (_, i) => String.fromCharCode(i)).join('')
  for (const length of [0, 1, 31, 32, 33, 63, 64, 65, 127, 128, 129]) {
    const input = '\ud800' + '中国'.repeat(length) + '\udc00"\\' + controls + '𠮷🙂é\ud800'
    const normalized = Buffer.from(input).toString('utf8')
    for (const style of styles) {
      for (const resolver of ['character', 'phrase', 'jieba']) {
        const options = {
          style,
          segment: resolver !== 'character',
          segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
        }
        const expected = pinyin(normalized, options)
        t.deepEqual(pinyin(input, options), expected)
        t.deepEqual(await asyncPinyin(input, options), expected)
        const nested = { ...options, heteronym: true }
        t.deepEqual(pinyin(input, nested), pinyin(normalized, nested))
        t.deepEqual(await asyncPinyin(input, nested), pinyin(normalized, nested))
        for (const separator of ['', ' ', '\0🙂', '\ud800']) {
          const normalizedSeparator = Buffer.from(separator).toString('utf8')
          t.is(pinyinString(input, { ...options, separator }), (expected as string[]).join(normalizedSeparator))
        }
      }
    }
  }
})

test('bulk arrays escape every control character and preserve fresh nested arrays', async (t) => {
  const controls = Array.from({ length: 32 }, (_, i) => String.fromCharCode(i)).join('')
  const input = ('中"\\' + controls + '\u2028\u2029🙂国').repeat(200)
  const unit = pinyin('中"\\' + controls + '\u2028\u2029🙂国')
  const expected = Array.from({ length: 200 }, () => unit).flat()
  t.deepEqual(pinyin(input), expected)
  t.deepEqual(await asyncPinyin(input), expected)
  const nested = pinyin(input, { heteronym: true }) as string[][]
  t.is(nested.length, expected.length)
  nested[0][0] = 'changed'
  t.not(nested[3][0], 'changed')
})

test('bulk parser references belong to each worker environment and are cleaned up', async (t) => {
  for (let batch = 0; batch < 3; batch++) {
    const outputs = await Promise.all(
      Array.from(
        { length: 4 },
        (_, i) =>
          new Promise<{ expected: unknown; sync: unknown; async: unknown }>((resolve, reject) => {
            const worker = new Worker(new URL('./helpers/worker.cjs', import.meta.url), {
              workerData: {
                multi: i % 2 === 0,
                style: (batch + i) % 5,
                segmenter: batch % 2 === 0 ? 'jieba' : 'phrase',
              },
            })
            let result: { expected: unknown; sync: unknown; async: unknown }
            worker.once('message', (value) => {
              result = value
            })
            worker.once('error', reject)
            worker.once('exit', (code) =>
              code === 0 && result ? resolve(result) : reject(new Error(`Worker exited ${code}`)),
            )
          }),
      ),
    )
    for (const output of outputs) {
      t.deepEqual(output.sync, output.expected)
      t.deepEqual(output.async, output.expected)
    }
  }
})
