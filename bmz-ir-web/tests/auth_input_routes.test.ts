import { afterAll, expect, mock, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { readdir, readFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { createApp, defineEventHandler, toNodeListener } from 'h3'
import * as schema from '../server/db/schema'
import requestBodyMiddleware from '../server/middleware/00.request-body'

const client = createClient({ url: 'file::memory:' })
const db = drizzle(client, { schema })
mock.module('hub:db', () => ({ db, schema }))
mock.module('../server/utils/auth', () => ({
  requireIrUser: async () => ({ id: 'owner', email: 'owner@example.invalid' }),
}))
mock.module('../server/utils/auth_rate_limit', () => ({ checkAuthRateLimit: async () => {} }))
mock.module('../server/utils/auth_tokens', () => ({
  createAuthTokens: async () => ({
    accessToken: 'test-access',
    refreshToken: 'test-refresh',
    accessExpiresAt: 1234567890,
    sessionGroupId: 'test-session',
  }),
}))
Object.assign(globalThis, {
  defineEventHandler,
  hashPassword: async () => 'test-password-hash',
  setUserSession: async () => {},
  replaceUserSession: async () => {},
})
const { default: register } = await import('../server/api/v1/auth/register.post')
const { default: profile } = await import('../server/api/v1/profile.put')
const migrations = new URL('../../server/db/migrations/sqlite/', import.meta.url)
for (const name of (await readdir(migrations)).filter((name) => name.endsWith('.sql')).sort()) {
  await client.executeMultiple(await readFile(new URL(name, migrations), 'utf8'))
}
await db
  .insert(schema.users)
  .values({ id: 'owner', email: 'owner@example.invalid', passwordHash: '' })
afterAll(() => client.close())

test('registration and profile updates share display-name boundaries and reject controls with 400', async () => {
  const app = createApp()
    .use(requestBodyMiddleware)
    .use('/register', register)
    .use('/profile', profile)
  const server = createServer(toNodeListener(app))
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as { port: number }
  let sequence = 0
  try {
    for (const route of ['register', 'profile']) {
      for (const name of [
        ' x ',
        `  ${'あ'.repeat(64)}  `,
        '',
        ' ',
        'a'.repeat(65),
        'a\nb',
        'a\x00b',
        'a\x7fb',
        'a\x85b',
      ]) {
        const valid =
          name.trim().length > 0 && name.trim().length <= 64 && !/\p{Cc}/u.test(name.trim())
        const response = await fetch(`http://127.0.0.1:${port}/${route}`, {
          method: route === 'register' ? 'POST' : 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            display_name: name,
            email: `new-${sequence++}@example.invalid`,
            password: '12345678',
          }),
        })
        expect(response.status).toBe(valid ? 200 : 400)
        if (valid) expect((await response.json()).player.display_name).toBe(name.trim())
      }
    }
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})
