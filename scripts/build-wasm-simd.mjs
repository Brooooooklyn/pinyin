import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'

const require = createRequire(import.meta.url)
const cliPackage = require('@napi-rs/cli/package.json')
const cli = resolve(dirname(require.resolve('@napi-rs/cli/package.json')), cliPackage.bin.napi)

// Keep the standard module intact. Consumers opt into the additional kernels by
// loading the generated entry in target/wasi-simd (or their chosen output dir).
const result = spawnSync(
  process.execPath,
  [
    cli,
    'build',
    '--platform',
    '--release',
    '--target',
    'wasm32-wasip1-threads',
    '--output-dir',
    'target/wasi-simd',
    ...process.argv.slice(2),
  ],
  {
    stdio: 'inherit',
    env: {
      ...process.env,
      RUSTFLAGS: [
        process.env.RUSTFLAGS || process.env.CARGO_TARGET_WASM32_WASIP1_THREADS_RUSTFLAGS || '',
        '-C target-feature=+simd128',
      ].join(' '),
    },
  },
)
if (result.error) throw result.error
process.exitCode = result.status ?? 1
