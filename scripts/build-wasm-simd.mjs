import { parseArgs } from 'node:util'
import { NapiCli } from '@napi-rs/cli'

const { values, positionals } = parseArgs({
  options: {
    'output-dir': { type: 'string', short: 'o', default: 'target/wasi-simd' },
    'target-dir': { type: 'string' },
  },
  allowPositionals: true,
})

const previousRustFlags = process.env.RUSTFLAGS
process.env.RUSTFLAGS = [
  previousRustFlags || process.env.CARGO_TARGET_WASM32_WASIP1_THREADS_RUSTFLAGS || '',
  '-C target-feature=+simd128',
].join(' ')

// Keep the standard module intact. Consumers opt into the additional kernels by
// loading the generated entry in target/wasi-simd (or their chosen output dir).
try {
  const { task } = await new NapiCli().build({
    platform: true,
    release: true,
    target: 'wasm32-wasip1-threads',
    outputDir: values['output-dir'],
    targetDir: values['target-dir'],
    cargoOptions: positionals,
  })
  await task
} finally {
  if (previousRustFlags === undefined) delete process.env.RUSTFLAGS
  else process.env.RUSTFLAGS = previousRustFlags
}
