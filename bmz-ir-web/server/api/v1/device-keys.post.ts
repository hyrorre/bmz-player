import { randomUUID } from 'node:crypto'
import { and, eq, isNull } from 'drizzle-orm'
import { createError, readBody } from 'h3'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

interface DeviceKeyBody {
  public_key?: string
  algorithm?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as DeviceKeyBody
  const publicKey = body?.public_key?.toLowerCase() ?? ''
  if (!/^[0-9a-f]{64}$/.test(publicKey)) {
    throw createError({ statusCode: 400, statusMessage: 'public_key must be 32 bytes hex' })
  }
  if ((body.algorithm ?? 'ed25519') !== 'ed25519') {
    throw createError({ statusCode: 400, statusMessage: 'unsupported algorithm' })
  }

  const existing = await db
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
  if (existing[0]) {
    return { id: existing[0].id, created: false }
  }

  const id = randomUUID()
  await db.insert(schema.deviceKeys).values({
    id,
    playerId: user.id,
    publicKey,
    algorithm: 'ed25519',
  })
  return { id, created: true }
})
