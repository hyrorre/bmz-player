import { desc, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const data = await db
    .select({
      id: schema.deviceKeys.id,
      public_key: schema.deviceKeys.publicKey,
      algorithm: schema.deviceKeys.algorithm,
      revoked_at: schema.deviceKeys.revokedAt,
      created_at: schema.deviceKeys.createdAt,
    })
    .from(schema.deviceKeys)
    .where(eq(schema.deviceKeys.playerId, user.id))
    .orderBy(desc(schema.deviceKeys.createdAt))
  return { device_keys: data }
})
