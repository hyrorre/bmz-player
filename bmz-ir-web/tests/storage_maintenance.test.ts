import { afterAll, beforeEach, expect, mock, spyOn, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { eq, isNull } from 'drizzle-orm'
import { createHash, generateKeyPairSync, sign } from 'node:crypto'
import { readdir, readFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import {
  createApp,
  createError,
  createRouter,
  defineEventHandler,
  getHeader,
  getRouterParam,
  toNodeListener,
  type H3Event,
} from 'h3'
import * as schema from '../server/db/schema'
import requestBodyMiddleware from '../server/middleware/00.request-body'
import type { IrScoreSubmission } from '../shared/types/ir'

const client = createClient({ url: 'file::memory:' })
let maxParameters = 0
const db = drizzle(client, {
  schema,
  logger: {
    logQuery(_query, parameters) {
      maxParameters = Math.max(maxParameters, parameters.length)
      if (parameters.length > 100) throw new Error('D1 parameter limit exceeded')
    },
  },
})
mock.module('hub:db', () => ({ db, schema }))
const objects = new Map<string, Uint8Array>()
const failingPaths = new Set<string>()
mock.module('hub:blob', () => ({
  blob: {
    del: async (paths: string | string[]) => {
      for (const path of Array.isArray(paths) ? paths : [paths]) {
        if (failingPaths.has(path)) throw new Error('fixture blob is unavailable')
        objects.delete(path)
      }
    },
  },
}))
const requestUser = (event: H3Event) => ({ id: getHeader(event, 'x-test-user') ?? 'owner' })
mock.module('../server/utils/auth', () => ({
  requireIrUser: async (event: H3Event) => requestUser(event),
  resolveIrUser: async (event: H3Event) => requestUser(event),
}))
Object.assign(globalThis, {
  defineEventHandler,
  getRouterParam,
  createError,
  defineTask: (task: unknown) => task,
})
const { default: createKey, MAX_ACTIVE_DEVICE_KEYS } =
  await import('../server/api/v1/device-keys.post')
const { default: scoreDetail } = await import('../server/api/v1/scores/[id]/index.get')
const { default: task } = await import('../server/tasks/ir/storage-cleanup')
const {
  cleanupIrStorage,
  STORAGE_CLEANUP_BATCH_SIZE,
  UNVERIFIED_REPLAY_RETENTION_MS,
  REVOKED_DEVICE_KEY_RETENTION_MS,
} = await import('../server/services/storage_cleanup')
const { DEVICE_KEY_CREATE_RATE_LIMIT } = await import('../server/utils/rate_limit')
const { submitScore } = await import('../server/services/ir/submission')
const { canonicalSubmissionJson, resolveVerification } =
  await import('../server/services/ir/verification')

const migrations = new URL('../../server/db/migrations/sqlite/', import.meta.url)
for (const name of (await readdir(migrations)).filter((name) => name.endsWith('.sql')).sort()) {
  await client.executeMultiple(await readFile(new URL(name, migrations), 'utf8'))
}
const users = ['owner', 'second', 'third', 'fourth', 'fifth']
for (const id of users) {
  await db.insert(schema.users).values({ id, email: `${id}@example.invalid`, passwordHash: '' })
  await db.insert(schema.profiles).values({ id })
}
const router = createRouter()
  .post('/api/v1/device-keys', createKey)
  .get('/api/v1/scores/:id', scoreDetail)
const server = createServer(toNodeListener(createApp().use(requestBodyMiddleware).use(router)))
await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
const { port } = server.address() as { port: number }
const root = `http://127.0.0.1:${port}`
afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()))
  client.close()
})
beforeEach(async () => {
  objects.clear()
  failingPaths.clear()
  await db.delete(schema.authRateLimits)
  await db.delete(schema.replayObjects)
  await db.delete(schema.bestScores)
  await db.delete(schema.scores)
  await db.delete(schema.deviceKeys)
  await db.delete(schema.taskLocks)
  maxParameters = 0
})
const publicKey = (number: number) => number.toString(16).padStart(64, '0')
const register = (key: string, user = 'owner') =>
  fetch(`${root}/api/v1/device-keys`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-test-user': user },
    body: JSON.stringify({ public_key: key }),
  })

test('device keys allow ten active keys, idempotent retries, revocation and atomic concurrent quota checks', async () => {
  for (let i = 0; i < MAX_ACTIVE_DEVICE_KEYS - 1; i++)
    expect((await register(publicKey(i))).status).toBe(200)
  const concurrent = await Promise.all([register(publicKey(20)), register(publicKey(21))])
  expect(concurrent.map((response) => response.status).sort()).toEqual([200, 409])
  const active = await db
    .select()
    .from(schema.deviceKeys)
    .where(isNull(schema.deviceKeys.revokedAt))
  expect(active).toHaveLength(MAX_ACTIVE_DEVICE_KEYS)
  expect((await register(active[0]!.publicKey)).status).toBe(200)
  const rejected = await register(publicKey(22))
  expect(rejected.status).toBe(409)
  expect((await rejected.json()).statusMessage).toContain('revoke an old key')
  await db
    .update(schema.deviceKeys)
    .set({ revokedAt: new Date() })
    .where(eq(schema.deviceKeys.id, active[0]!.id))
  const duplicate = await Promise.all([register(publicKey(23)), register(publicKey(23))])
  expect(duplicate.map((response) => response.status)).toEqual([200, 200])
  expect(
    await db.select().from(schema.deviceKeys).where(isNull(schema.deviceKeys.revokedAt)),
  ).toHaveLength(MAX_ACTIVE_DEVICE_KEYS)
})

test('device-key creation enforces both user and IP rate limits with 429', async () => {
  for (let i = 0; i < DEVICE_KEY_CREATE_RATE_LIMIT.user; i++)
    expect((await register(publicKey(1))).status).toBe(200)
  expect((await register(publicKey(1))).status).toBe(429)
  await db.delete(schema.authRateLimits)
  for (let i = 0; i < DEVICE_KEY_CREATE_RATE_LIMIT.ip; i++) {
    expect((await register(publicKey(1), users[i % users.length])).status).toBe(200)
  }
  expect((await register(publicKey(1), 'second')).status).toBe(429)
})

function submission(key: string): IrScoreSubmission {
  const zero = { pgreat: 0, great: 0, good: 0, bad: 0, poor: 0, empty_poor: 0 }
  return {
    client: { name: 'BMZ', version: 'test', platform: 'windows' },
    chart: { sha256: 'a'.repeat(64) },
    rule: {
      play_mode: 'normal',
      key_mode: '7k',
      gauge: 'Normal',
      ln_policy: 'ForceLn',
      effective_ln_mode: 'ln',
      rule_mode: 'Beatoraja',
      judge_algorithm: 'combo',
      scoring: 'bms_ex_score_v1',
    },
    result: {
      clear: 'Normal',
      notes: 1,
      ex_score: 2,
      max_combo: 1,
      min_bp: 0,
      min_cb: 0,
      pass_notes: 1,
      judges: { fast: { ...zero, pgreat: 1 }, slow: zero },
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: key,
  }
}

async function replay(
  name: string,
  status: (typeof schema.replayObjects.$inferInsert)['status'],
  updatedAt: Date,
) {
  const score = await submitScore({ id: 'owner' }, submission(name), [], 10)
  const path = `owner/${score.score_id}/${name}`
  await db.insert(schema.replayObjects).values({
    id: name,
    scoreId: score.score_id!,
    playerId: 'owner',
    hash: 'a'.repeat(64),
    format: 'bmz-replay-v1',
    objectPath: path,
    status,
    updatedAt,
  })
  objects.set(path, new TextEncoder().encode(name))
  return path
}

test('scheduled cleanup removes only expired pending/uploaded blobs and retains verification records', async () => {
  const now = new Date()
  const old = new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS - 1)
  const boundary = new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS)
  const removed = [
    await replay('old-pending', 'pending_upload', old),
    await replay('old-uploaded', 'uploaded', old),
    await replay('rejected', 'rejected', now),
  ]
  const retained = [
    await replay('new-pending', 'pending_upload', now),
    await replay('boundary', 'uploaded', boundary),
    await replay('verified', 'verified', old),
    await replay('metadata-only', 'metadata_only', old),
  ]
  const result = await cleanupIrStorage(now)
  expect(result).toEqual({ expired_replays: 2, deleted_replay_blobs: 3, deleted_device_keys: 0 })
  for (const path of removed) expect(objects.has(path)).toBe(false)
  for (const path of retained) expect(objects.has(path)).toBe(true)
  expect(await db.select().from(schema.replayObjects)).toHaveLength(7)
  expect(
    (await db.query.replayObjects.findFirst({ where: eq(schema.replayObjects.id, 'verified') }))
      ?.status,
  ).toBe('verified')
})

test('cleanup batches and task locks are bounded below the D1 parameter limit', async () => {
  const old = new Date(Date.now() - UNVERIFIED_REPLAY_RETENTION_MS - 1000)
  const revoked = new Date(Date.now() - REVOKED_DEVICE_KEY_RETENTION_MS - 1000)
  for (let i = 0; i < STORAGE_CLEANUP_BATCH_SIZE + 1; i++) {
    await replay(`batch-${i}`, 'uploaded', old)
    await db
      .insert(schema.deviceKeys)
      .values({ id: `key-${i}`, playerId: 'owner', publicKey: publicKey(i), revokedAt: revoked })
  }
  const event = { name: 'ir:storage-cleanup', payload: {}, context: {} }
  const first = await task.run(event)
  expect(first.result).toEqual({
    skipped: false,
    expired_replays: STORAGE_CLEANUP_BATCH_SIZE,
    deleted_replay_blobs: STORAGE_CLEANUP_BATCH_SIZE,
    deleted_device_keys: STORAGE_CLEANUP_BATCH_SIZE,
  })
  expect(objects.size).toBe(1)
  expect((await cleanupIrStorage()).deleted_replay_blobs).toBe(1)
  expect(maxParameters).toBeLessThanOrEqual(100)
  await db.insert(schema.taskLocks).values({
    name: 'ir:storage-cleanup',
    owner: 'other',
    leaseUntil: new Date(Date.now() + 60_000),
  })
  expect((await task.run(event)).result).toEqual({ skipped: true, reason: 'already_running' })
})

test('one failed blob deletion does not block other rows and is retried later', async () => {
  const warning = spyOn(console, 'warn').mockImplementation(() => {})
  try {
    const failed = await replay('failed-delete', 'rejected', new Date())
    const successful = await replay('successful-delete', 'rejected', new Date())
    failingPaths.add(failed)
    expect((await cleanupIrStorage()).deleted_replay_blobs).toBe(1)
    expect(objects.has(failed)).toBe(true)
    expect(objects.has(successful)).toBe(false)
    failingPaths.clear()
    expect((await cleanupIrStorage()).deleted_replay_blobs).toBe(1)
    expect(objects.size).toBe(0)
  } finally {
    warning.mockRestore()
  }
})

test('purging 90-day revoked keys leaves signed score verification and public details unchanged', async () => {
  const keys = generateKeyPairSync('ed25519')
  const keyBytes = keys.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)
  const registered = await (await register(keyBytes.toString('hex'))).json()
  const payload = submission('signed-history')
  const hash = createHash('sha256').update(canonicalSubmissionJson(payload)).digest()
  payload.evidence = {
    schema: 'bmz-score-evidence-v1',
    public_key_id: registered.id,
    canonical_hash: hash.toString('hex'),
    client_signature: sign(null, hash, keys.privateKey).toString('base64url'),
  }
  const score = await submitScore({ id: 'owner' }, payload, [], 10)
  const now = new Date()
  const cutoff = now.getTime() - REVOKED_DEVICE_KEY_RETENTION_MS
  await db
    .update(schema.deviceKeys)
    .set({ revokedAt: new Date(cutoff - 1) })
    .where(eq(schema.deviceKeys.id, registered.id))
  await db.insert(schema.deviceKeys).values({
    id: 'boundary',
    playerId: 'owner',
    publicKey: publicKey(99),
    revokedAt: new Date(cutoff),
  })
  await db
    .insert(schema.deviceKeys)
    .values({ id: 'active', playerId: 'owner', publicKey: publicKey(100) })
  await expect(resolveVerification('owner', payload)).rejects.toThrow('evidence is invalid')
  expect((await cleanupIrStorage(now)).deleted_device_keys).toBe(1)
  expect((await db.select().from(schema.deviceKeys)).map((key) => key.id).sort()).toEqual([
    'active',
    'boundary',
  ])
  await expect(resolveVerification('owner', payload)).rejects.toThrow('evidence is invalid')
  const detail = await (await fetch(`${root}/api/v1/scores/${score.score_id}`)).json()
  expect(detail.score.verification).toBe('verified_play')
  expect(detail.score.ex_score).toBe(2)
  expect((await db.select().from(schema.bestScores))[0]?.verification).toBe('verified_play')
})
