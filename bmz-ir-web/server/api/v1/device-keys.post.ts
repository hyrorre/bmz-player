import { randomUUID } from 'node:crypto'
import { and, eq, isNull, sql } from 'drizzle-orm'
import { createError, readBody } from 'h3'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'
import { DEVICE_KEY_CREATE_RATE_LIMIT, checkUserRateLimit } from '../../utils/rate_limit'

export const MAX_ACTIVE_DEVICE_KEYS = 10

interface DeviceKeyBody {
  public_key?: string
  algorithm?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'device_key_create', user.id, DEVICE_KEY_CREATE_RATE_LIMIT)
  const body = (await readBody(event)) as DeviceKeyBody
  const publicKey = typeof body?.public_key === 'string' ? body.public_key.toLowerCase() : ''
  if (!/^[0-9a-f]{64}$/.test(publicKey)) {
    throw createError({ statusCode: 400, statusMessage: 'public_key must be 32 bytes hex' })
  }
  if ((body.algorithm ?? 'ed25519') !== 'ed25519') {
    throw createError({ statusCode: 400, statusMessage: 'unsupported algorithm' })
  }

  const findExisting = () =>
    db
      .select({ id: schema.deviceKeys.id })
      .from(schema.deviceKeys)
      .where(
        and(
          eq(schema.deviceKeys.playerId, user.id),
          eq(schema.deviceKeys.publicKey, publicKey),
          isNull(schema.deviceKeys.revokedAt),
        ),
      )
      .limit(1)
  const existing = await findExisting()
  if (existing[0]) {
    return { id: existing[0].id, created: false }
  }

  const id = randomUUID()
  // Count and insert in one statement: concurrent requests cannot both claim
  // the tenth slot, or create duplicate active registrations of the same key.
  const created = await db
    .insert(schema.deviceKeys)
    .select(
      sql`
    SELECT ${id}, ${user.id}, ${publicKey}, 'ed25519', NULL, ${Date.now()}
    WHERE (SELECT count(*) FROM ${schema.deviceKeys}
      WHERE ${schema.deviceKeys.playerId} = ${user.id}
      AND ${schema.deviceKeys.revokedAt} IS NULL) < ${MAX_ACTIVE_DEVICE_KEYS}
    AND NOT EXISTS (SELECT 1 FROM ${schema.deviceKeys}
      WHERE ${schema.deviceKeys.playerId} = ${user.id}
      AND ${schema.deviceKeys.publicKey} = ${publicKey}
      AND ${schema.deviceKeys.revokedAt} IS NULL)
  `,
    )
    .returning({ id: schema.deviceKeys.id })
  if (created.length === 0) {
    const concurrent = await findExisting()
    if (concurrent[0]) return { id: concurrent[0].id, created: false }
    throw createError({
      statusCode: 409,
      statusMessage: 'Active device key limit reached; revoke an old key before adding another',
    })
  }
  return { id, created: true }
})
