// Strict WASI selection applies to this binding, not the TypeScript loader or
// other native development dependencies that may have no installed WASI build.
const forceWasi = process.env.PINYIN_TEST_WASI === 'true'
const previous = process.env.NAPI_RS_FORCE_WASI
if (forceWasi) process.env.NAPI_RS_FORCE_WASI = 'error'
try {
  module.exports = process.env.PINYIN_TEST_BINDING
    ? require(require('node:path').resolve(process.env.PINYIN_TEST_BINDING))
    : require('../../index.js')
} finally {
  if (forceWasi) {
    if (previous === undefined) delete process.env.NAPI_RS_FORCE_WASI
    else process.env.NAPI_RS_FORCE_WASI = previous
  }
}
