import { spawn } from 'node:child_process'
import { readFileSync, realpathSync } from 'node:fs'
import { setTimeout } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { stripVTControlCharacters } from 'node:util'

// Yarn 4.18 bundles Got's retry/cancellation race. Retry the install in a fresh
// process only for this uncaught error, not lockfile, checksum, or build errors.
// https://github.com/sindresorhus/got/issues/1489
export function isRetryableCrash(output) {
  return /^Error: The `onCancel` handler was attached after the promise settled\.\r?$/m.test(
    stripVTControlCharacters(output),
  )
}

export async function installWithRetry(run, pause = setTimeout, warn = console.warn) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    const result = await run()
    if (result.code === 0 || !result.retryable || attempt === 3) return result.code
    warn(`Yarn hit its HTTP cancellation race (attempt ${attempt}/3); retrying dependency installation.`)
    await pause(attempt * 5000)
  }
}

function install(args) {
  const root = new URL('../../', import.meta.url)
  const { packageManager } = JSON.parse(readFileSync(new URL('package.json', root), 'utf8'))
  const version = /^yarn@(\d+\.\d+\.\d+(?:-[\w.]+)?)(?:\+.*)?$/.exec(packageManager)?.[1]
  if (!version) throw new Error('Expected a pinned Yarn version in package.json#packageManager')
  const yarn = fileURLToPath(new URL(`.yarn/releases/yarn-${version}.cjs`, root))

  return new Promise((resolve, reject) => {
    let retryable = false
    const child = spawn(process.execPath, [yarn, 'install', '--immutable', ...args], {
      cwd: fileURLToPath(root),
      stdio: ['inherit', 'pipe', 'pipe'],
    })
    for (const [stream, destination] of [
      [child.stdout, process.stdout],
      [child.stderr, process.stderr],
    ]) {
      let tail = ''
      stream.on('data', (chunk) => {
        destination.write(chunk)
        const output = tail + chunk.toString()
        retryable ||= isRetryableCrash(output)
        // Keep a bounded overlap to recognize a diagnostic split across chunks.
        tail = output.slice(-512)
      })
    }
    child.once('error', reject)
    child.once('close', (code) => resolve({ code: code ?? 1, retryable: code !== null && retryable }))
  })
}

if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(process.argv[1])) {
  try {
    process.exitCode = await installWithRetry(() => install(process.argv.slice(2)))
  } catch (error) {
    console.error(error)
    process.exitCode = 1
  }
}
