const { execSync } = require('child_process')
const { readFileSync, writeFileSync } = require('fs')
const { join } = require('path')
const { platform, arch } = require('os')

const { platformArchTriples } = require('@napi-rs/triples')

const PLATFORM_NAME = platform()
const ARCH_NAME = arch()

if (process.env.npm_config_build_from_source || process.env.BUILD_PINYIN_FROM_SOURCE) {
  let libExt
  let dylibName = 'napi_pinyin'
  switch (PLATFORM_NAME) {
    case 'darwin':
      libExt = '.dylib'
      dylibName = `lib${dylibName}`
      break
    case 'win32':
      libExt = '.dll'
      break
    case 'linux':
    case 'freebsd':
    case 'openbsd':
    case 'android':
    case 'sunos':
      dylibName = `lib${dylibName}`
      libExt = '.so'
      break
    default:
      throw new TypeError('Operating system not currently supported or recognized by the build script')
  }
  execSync('Cargo build --release', {
    stdio: 'inherit',
    env: process.env,
  })
  const dylibContent = readFileSync(join(__dirname, 'target', 'release', `${dylibName}${libExt}`))
  const triples = platformArchTriples[PLATFORM_NAME][ARCH_NAME]
  const tripe = triples[0]
  writeFileSync(join(__dirname, `pinyin.${tripe.platformArchABI}.node`), dylibContent)
}
