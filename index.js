const { loadBinding } = require('@node-rs/helper')

/**
 * __dirname means load native addon from current dir
 * 'pinyin' means native addon name is `pinyin`
 * the first arguments was decided by `napi.name` field in `package.json`
 * the second arguments was decided by `name` field in `package.json`
 * loadBinding helper will load `pinyin.[PLATFORM].node` from `__dirname` first
 * If failed to load addon, it will fallback to load from `@napi-rs/pinyin-[PLATFORM]`
 */
const bindings = loadBinding(__dirname, 'pinyin', '@napi-rs/pinyin')

function pinyin(input, options = {}) {
  return bindings.pinyin(
    input,
    typeof options.heteronym === 'undefined' ? false : options.heteronym,
    typeof options.style === 'undefined' ? bindings.PINYIN_STYLE.WithTone : options.style,
    typeof options.heteronym === 'undefined' ? false : options.heteronym,
  )
}

function asyncPinyin(input, options = {}) {
  return bindings.asyncPinyin(
    input,
    typeof options.heteronym === 'undefined' ? false : options.heteronym,
    typeof options.style === 'undefined' ? bindings.PINYIN_STYLE.WithTone : options.style,
    typeof options.heteronym === 'undefined' ? false : options.heteronym,
  )
}

module.exports = {
  pinyin,
  asyncPinyin,
  compare: bindings.compare,
  PINYIN_STYLE: bindings.PINYIN_STYLE,
}
