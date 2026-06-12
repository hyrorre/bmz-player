import { and, eq, isNull } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const keyId = getRouterParam(event, 'id')
  if (!keyId) {
    throw createError({ statusCode: 400, statusMessage: 'device key id is required' })
  }

  const existing = await db
    .select({ id: schema.deviceKeys.id })
    .from(schema.deviceKeys)
    .where(
      and(
        eq(schema.deviceKeys.id, keyId),
        eq(schema.deviceKeys.playerId, user.id),
        isNull(schema.deviceKeys.revokedAt),
      ),
    )
    .limit(1)
  if (!existing[0]) {
    throw createError({ statusCode: 404, statusMessage: 'Device key not found or already revoked' })
  }

  await db
    .update(schema.deviceKeys)
    .set({ revokedAt: new Date() })
    .where(eq(schema.deviceKeys.id, keyId))
  return { revoked: true, id: existing[0].id }
})
