import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync, symlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { installWithRetry, isRetryableCrash } from './yarn-install.mjs'

const crash = 'Error: The `onCancel` handler was attached after the promise settled.'

test('CLI entrypoint runs through a symlinked checkout', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'pinyin-yarn-install-'))
  try {
    const checkout = join(temporary, 'checkout')
    symlinkSync(fileURLToPath(new URL('../../', import.meta.url)), checkout, 'junction')
    const output = execFileSync(process.execPath, [join(checkout, '.github/scripts/yarn-install.mjs'), '--help'], {
      encoding: 'utf8',
    })
    assert.match(output, /yarn install/)
  } finally {
    rmSync(temporary, { recursive: true, force: true })
  }
})

test('recognizes the uncaught diagnostic, not the bundled source literal', () => {
  assert.ok(isRetryableCrash(`\u001b[31m${crash}\u001b[0m\r\n    at onCancel (yarn.cjs:143:21119)`))
  assert.ok(!isRetryableCrash(`const message = '${crash}';\nError: a different failure`))
  assert.ok(!isRetryableCrash('YN0028: The lockfile would have been modified by this install'))
  assert.ok(!isRetryableCrash('YN0018: The remote archive does not match the expected checksum'))
})

async function simulate(results) {
  let calls = 0
  const pauses = []
  const warnings = []
  const code = await installWithRetry(
    async () => {
      assert.ok(calls < results.length, 'unexpected extra install attempt')
      return results[calls++]
    },
    async (ms) => pauses.push(ms),
    (message) => warnings.push(message),
  )
  return { code, calls, pauses, warnings }
}

test('successful installs are run once', async () => {
  const result = await simulate([{ code: 0, retryable: false }])
  assert.equal(result.code, 0)
  assert.equal(result.calls, 1)
  assert.deepEqual(result.pauses, [])
})

test('ordinary failures preserve their exit status without retrying', async () => {
  const result = await simulate([{ code: 42, retryable: false }])
  assert.equal(result.code, 42)
  assert.equal(result.calls, 1)
  assert.deepEqual(result.warnings, [])
})

test('the cancellation race retries with backoff and may recover', async () => {
  const result = await simulate([
    { code: 1, retryable: true },
    { code: 1, retryable: true },
    { code: 0, retryable: false },
  ])
  assert.equal(result.code, 0)
  assert.equal(result.calls, 3)
  assert.deepEqual(result.pauses, [5000, 10000])
  assert.equal(result.warnings.length, 2)
})

test('persistent cancellation failures still fail after three attempts', async () => {
  const result = await simulate(Array.from({ length: 3 }, () => ({ code: 1, retryable: true })))
  assert.equal(result.code, 1)
  assert.equal(result.calls, 3)
  assert.deepEqual(result.pauses, [5000, 10000])
})

test('a real error after a transient crash stops further retries', async () => {
  const result = await simulate([
    { code: 1, retryable: true },
    { code: 2, retryable: false },
  ])
  assert.equal(result.code, 2)
  assert.equal(result.calls, 2)
  assert.deepEqual(result.pauses, [5000])
})
