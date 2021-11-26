const { existsSync, readFileSync } = require('fs')
const { join } = require('path')

const { platform, arch } = process

let nativeBinding = null
let localFileExisted = false
let isMusl = false
let loadError = null

switch (platform) {
  case 'android':
    if (arch !== 'arm64') {
      throw new Error(`Unsupported architecture on Android ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'pinyin.android-arm64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./pinyin.android-arm64.node')
      } else {
        nativeBinding = require('@napi-rs/pinyin-android-arm64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'win32':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(join(__dirname, 'pinyin.win32-x64-msvc.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.win32-x64-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-win32-x64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'ia32':
        localFileExisted = existsSync(join(__dirname, 'pinyin.win32-ia32-msvc.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.win32-ia32-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-win32-ia32-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(join(__dirname, 'pinyin.win32-arm64-msvc.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.win32-arm64-msvc.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-win32-arm64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Windows: ${arch}`)
    }
    break
  case 'darwin':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(join(__dirname, 'pinyin.darwin-x64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.darwin-x64.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-darwin-x64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(join(__dirname, 'pinyin.darwin-arm64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.darwin-arm64.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-darwin-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on macOS: ${arch}`)
    }
    break
  case 'freebsd':
    if (arch !== 'x64') {
      throw new Error(`Unsupported architecture on FreeBSD: ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'pinyin.freebsd-x64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./pinyin.freebsd-x64.node')
      } else {
        nativeBinding = require('@napi-rs/pinyin-freebsd-x64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'linux':
    switch (arch) {
      case 'x64':
        isMusl = readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
        if (isMusl) {
          localFileExisted = existsSync(join(__dirname, 'pinyin.linux-x64-musl.node'))
          try {
            if (localFileExisted) {
              nativeBinding = require('./pinyin.linux-x64-musl.node')
            } else {
              nativeBinding = require('@napi-rs/pinyin-linux-x64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(join(__dirname, 'pinyin.linux-x64-gnu.node'))
          try {
            if (localFileExisted) {
              nativeBinding = require('./pinyin.linux-x64-gnu.node')
            } else {
              nativeBinding = require('@napi-rs/pinyin-linux-x64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm64':
        isMusl = readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
        if (isMusl) {
          localFileExisted = existsSync(join(__dirname, 'pinyin.linux-arm64-musl.node'))
          try {
            if (localFileExisted) {
              nativeBinding = require('./pinyin.linux-arm64-musl.node')
            } else {
              nativeBinding = require('@napi-rs/pinyin-linux-arm64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(join(__dirname, 'pinyin.linux-arm64-gnu.node'))
          try {
            if (localFileExisted) {
              nativeBinding = require('./pinyin.linux-arm64-gnu.node')
            } else {
              nativeBinding = require('@napi-rs/pinyin-linux-arm64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm':
        localFileExisted = existsSync(join(__dirname, 'pinyin.linux-arm-gnueabihf.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./pinyin.linux-arm-gnueabihf.node')
          } else {
            nativeBinding = require('@napi-rs/pinyin-linux-arm-gnueabihf')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Linux: ${arch}`)
    }
    break
  default:
    throw new Error(`Unsupported OS: ${platform}, architecture: ${arch}`)
}

if (!nativeBinding) {
  if (loadError) {
    throw loadError
  }
  throw new Error(`Failed to load native binding`)
}

const { PINYIN_STYLE, pinyin, asyncPinyin, compare } = nativeBinding

module.exports.PINYIN_STYLE = PINYIN_STYLE
module.exports.pinyin = pinyin
module.exports.asyncPinyin = asyncPinyin
module.exports.compare = compare
