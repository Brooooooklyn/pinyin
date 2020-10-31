const { strictEqual } = require('assert')

const { pinyin, asyncPinyin } = require('./index')

const pinyinResult = ['zhong', 'guo', 'ren']
for (const [index, py] of pinyin('中国人').entries()) {
  strictEqual(py, pinyinResult[index])
}

console.info('Simple test passed')
