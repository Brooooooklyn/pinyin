const { strictEqual } = require('assert')

const { pinyin, asyncPinyin } = require('./index')

const pinyinResult = ['zhōng', 'guó', 'rén']
for (const [index, py] of pinyin('中国人').entries()) {
  strictEqual(py, pinyinResult[index])
}

asyncPinyin('中国人').then((v) => {
  for (const [index, py] of v.entries()) {
    strictEqual(py, pinyinResult[index])
  }
  console.info('Simple test passed')
})
