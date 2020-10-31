const { loadBinding } = require('@node-rs/helper')

/**
 * __dirname means load native addon from current dir
 * 'pinyin' means native addon name is `pinyin`
 * the first arguments was decided by `napi.name` field in `package.json`
 * the second arguments was decided by `name` field in `package.json`
 * loadBinding helper will load `pinyin.[PLATFORM].node` from `__dirname` first
 * If failed to load addon, it will fallback to load from `@napi-rs/pinyin-[PLATFORM]`
 */
module.exports = loadBinding(__dirname, 'pinyin', '@napi-rs/pinyin')
