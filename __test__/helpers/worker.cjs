const { parentPort, workerData } = require('node:worker_threads')
const { pinyin, asyncPinyin } = require('./binding.cjs')

async function main() {
  const options = {
    segment: true,
    segmenter: workerData.segmenter,
    heteronym: workerData.multi,
    style: workerData.style,
  }
  const input = '重庆银行音乐，𠮷\0🙂'.repeat(200)
  const expected = pinyin(input, options)
  // The binding must retain its own intrinsic, scoped to this worker's Env.
  JSON.parse = () => {
    throw new Error('modified JSON.parse must not be called')
  }
  String.prototype.split = () => {
    throw new Error('modified split must not be called')
  }
  parentPort.postMessage({ expected, sync: pinyin(input, options), async: await asyncPinyin(input, options) })
}

main().catch((error) => {
  throw error
})
