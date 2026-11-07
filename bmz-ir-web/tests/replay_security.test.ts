import { afterAll, beforeEach, expect, mock, spyOn, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { eq } from 'drizzle-orm'
import { createHash } from 'node:crypto'
import { readdir, readFile } from 'node:fs/promises'
import { createError, type H3Event } from 'h3'
import * as schema from '../server/db/schema'

const client = createClient({ url: 'file::memory:' })
const db = drizzle(client, { schema })
mock.module('hub:db', () => ({ db, schema }))
mock.module('../server/utils/auth', () => ({ requireIrUser: async () => ({ id: 'owner' }) }))
mock.module('../server/utils/rate_limit', () => ({
  REPLAY_UPLOAD_RATE_LIMIT: {},
  checkUserRateLimit: async () => {},
}))
const objects = new Map<string, Uint8Array>()
let onRead: (() => Promise<void>) | undefined
let onDelete: (() => Promise<void>) | undefined
let failDelete = false
mock.module('hub:blob', () => ({
  blob: {
    put: async (path: string, bytes: Uint8Array) => {
      objects.set(path, bytes.slice())
    },
    del: async (paths: string | string[]) => {
      if (failDelete) throw new Error('fixture storage unavailable')
      await onDelete?.()
      for (const path of Array.isArray(paths) ? paths : [paths]) objects.delete(path)
    },
    get: async (path: string) => {
      const bytes = objects.get(path)?.slice()
      await onRead?.()
      return bytes ? new Blob([bytes]) : null
    },
  },
}))
mock.module('../server/utils/replay_body', () => ({
  readReplayBody: async (event: { body: Promise<Uint8Array> }) => event.body,
}))
Object.assign(globalThis, {
  defineEventHandler: (fn: unknown) => fn,
  getRouterParam: () => 'score',
  getQuery: (event: { uploadId: string }) => ({ upload_id: event.uploadId }),
  getRequestURL: () => new URL('https://ir.example.invalid/api'),
  createError,
})
const { default: uploadUrl } = await import('../server/api/v1/scores/[id]/replay/upload-url.post')
const { default: upload } = await import('../server/api/v1/scores/[id]/replay/upload.put')
const { default: verify } = await import('../server/api/v1/scores/[id]/replay/verify.post')
const { cleanupIrStorage, UNVERIFIED_REPLAY_RETENTION_MS } =
  await import('../server/services/storage_cleanup')
const hash = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex')
const good = new TextEncoder().encode('original replay')
const bad = new TextEncoder().encode('different replay')
const migrations = new URL('../../server/db/migrations/sqlite/', import.meta.url)
for (const name of (await readdir(migrations)).filter((n) => n.endsWith('.sql')).sort()) {
  await client.executeMultiple(await readFile(new URL(name, migrations), 'utf8'))
}
await db
  .insert(schema.users)
  .values({ id: 'owner', email: 'dummy@example.invalid', passwordHash: '' })
await db.insert(schema.profiles).values({ id: 'owner' })
await db.insert(schema.charts).values({ sha256: 'a'.repeat(64), mode: '7k' })
await db.insert(schema.scores).values({
  id: 'score',
  playerId: 'owner',
  chartSha256: 'a'.repeat(64),
  clientName: 'test',
  clientVersion: 'test',
  platform: 'linux',
  playMode: 'normal',
  keyMode: '7k',
  gauge: 'Normal',
  lnPolicy: 'ForceLn',
  effectiveLnMode: 'ln',
  ruleMode: 'Beatoraja',
  judgeAlgorithm: 'combo',
  scoring: 'bms_ex_score_v1',
  clearType: 'Failed',
  clearRank: 1,
  judges: { fast: {}, slow: {} } as never,
  exScore: 0,
  maxCombo: 0,
  notes: 1,
  bp: 1,
  cb: 1,
  minBp: 1,
  minCb: 1,
  passNotes: 0,
  deviceType: 'keyboard',
  idempotencyKey: 'test',
  replayHash: hash(good),
  replayFormat: 'bmz-replay-v1',
})
afterAll(() => client.close())
beforeEach(async () => {
  onRead = undefined
  onDelete = undefined
  failDelete = false
  objects.clear()
  await db.delete(schema.replayObjects)
})
const event = (uploadId = '', body = Promise.resolve(good)) =>
  ({ uploadId, body }) as unknown as H3Event
const intent = async () =>
  new URL((await uploadUrl(event())).upload_url).searchParams.get('upload_id')!
const current = () =>
  db.query.replayObjects.findFirst({ where: eq(schema.replayObjects.scoreId, 'score') })

test('a delayed concurrent PUT cannot replace verified bytes', async () => {
  const id = await intent()
  const body = Promise.withResolvers<Uint8Array>()
  const requested = Promise.withResolvers<void>()
  const late = upload({
    uploadId: id,
    get body() {
      requested.resolve()
      return body.promise
    },
  } as unknown as H3Event)
  await requested.promise
  await upload(event(id))
  const snapshot = Promise.withResolvers<void>()
  const finish = Promise.withResolvers<void>()
  onRead = async () => {
    snapshot.resolve()
    await finish.promise
  }
  const checking = verify(event())
  await snapshot.promise
  body.resolve(bad)
  await expect(late).rejects.toMatchObject({ statusCode: 409 })
  finish.resolve()
  expect((await checking).status).toBe('verified')
  const row = (await current())!
  expect(row.status).toBe('verified')
  expect(hash(objects.get(row.objectPath!)!)).toBe(row.hash)
  expect(objects.size).toBe(1)
})

test('verification of an earlier generation cannot modify a new intent', async () => {
  await upload(event(await intent()))
  const snapshot = Promise.withResolvers<void>()
  const finish = Promise.withResolvers<void>()
  onRead = async () => {
    snapshot.resolve()
    await finish.promise
  }
  const checking = verify(event())
  await snapshot.promise
  const next = await intent()
  finish.resolve()
  await expect(checking).rejects.toMatchObject({ statusCode: 409 })
  expect((await current())?.status).toBe('pending_upload')
  expect((await current())?.objectPath?.endsWith(next)).toBe(true)
  expect((await current())?.sizeBytes).toBeNull()
  expect(objects.size).toBe(0)
})

test('stale upload URLs are rejected and the current intent can be uploaded and verified repeatedly', async () => {
  const stale = await intent()
  const id = await intent()
  await expect(upload(event(stale))).rejects.toMatchObject({ statusCode: 409 })
  await expect(verify(event())).rejects.toMatchObject({ statusCode: 409 })
  expect(objects.size).toBe(0)
  await upload(event(id))
  expect((await verify(event())).status).toBe('verified')
  expect((await verify(event())).status).toBe('verified')
})

test('a mismatching replay is rejected', async () => {
  await upload(event(await intent(), Promise.resolve(bad)))
  expect((await verify(event())).status).toBe('rejected')
  expect((await current())?.status).toBe('rejected')
  expect((await current())?.objectPath).toBeNull()
  expect(objects.size).toBe(0)
})

test('rejected deletion failures retain a reference for scheduled retry', async () => {
  const warning = spyOn(console, 'warn').mockImplementation(() => {})
  try {
    await upload(event(await intent(), Promise.resolve(bad)))
    failDelete = true
    expect((await verify(event())).status).toBe('rejected')
    expect((await current())?.objectPath).not.toBeNull()
    expect(objects.size).toBe(1)
    failDelete = false
    expect((await cleanupIrStorage()).deleted_replay_blobs).toBe(1)
    expect(objects.size).toBe(0)
    expect((await current())?.status).toBe('rejected')
    expect((await current())?.objectPath).toBeNull()
  } finally {
    warning.mockRestore()
  }
})

test('expiration cannot be undone by verification already reading the old blob', async () => {
  await upload(event(await intent()))
  const now = new Date()
  await db
    .update(schema.replayObjects)
    .set({ updatedAt: new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS - 1) })
  const snapshot = Promise.withResolvers<void>()
  const finish = Promise.withResolvers<void>()
  onRead = async () => {
    snapshot.resolve()
    await finish.promise
  }
  const checking = verify(event())
  await snapshot.promise
  expect((await cleanupIrStorage(now)).expired_replays).toBe(1)
  finish.resolve()
  await expect(checking).rejects.toMatchObject({ statusCode: 409 })
  expect((await current())?.status).toBe('rejected')
  expect(objects.size).toBe(0)
})

test('an expired pending upload rejects a late PUT and deletes its unadopted bytes', async () => {
  const id = await intent()
  const now = new Date()
  await db
    .update(schema.replayObjects)
    .set({ updatedAt: new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS - 1) })
  const body = Promise.withResolvers<Uint8Array>()
  const requested = Promise.withResolvers<void>()
  const late = upload({
    uploadId: id,
    get body() {
      requested.resolve()
      return body.promise
    },
  } as unknown as H3Event)
  await requested.promise
  await cleanupIrStorage(now)
  body.resolve(good)
  await expect(late).rejects.toMatchObject({ statusCode: 409 })
  expect((await current())?.status).toBe('rejected')
  expect(objects.size).toBe(0)
})

test('cleanup cannot clear or delete a renewed generation during blob deletion', async () => {
  await upload(event(await intent()))
  const now = new Date()
  await db
    .update(schema.replayObjects)
    .set({ updatedAt: new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS - 1) })
  onDelete = async () => {
    onDelete = undefined
    await upload(event(await intent()))
  }
  await cleanupIrStorage(now)
  expect((await current())?.status).toBe('uploaded')
  expect(objects.size).toBe(1)
  expect((await verify(event())).status).toBe('verified')
})

test('a PUT from a superseded intent cannot modify the new pending generation', async () => {
  const id = await intent()
  const body = Promise.withResolvers<Uint8Array>()
  const requested = Promise.withResolvers<void>()
  const late = upload({
    uploadId: id,
    get body() {
      requested.resolve()
      return body.promise
    },
  } as unknown as H3Event)
  await requested.promise
  const next = await intent()
  body.resolve(good)
  await expect(late).rejects.toMatchObject({ statusCode: 409 })
  expect((await current())?.status).toBe('pending_upload')
  expect((await current())?.objectPath?.endsWith(next)).toBe(true)
  expect(objects.size).toBe(0)
  await upload(event(next))
  expect((await verify(event())).status).toBe('verified')
})
