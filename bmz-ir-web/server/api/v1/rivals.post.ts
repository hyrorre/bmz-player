import { and, eq } from 'drizzle-orm'
import { readBody } from 'h3'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

interface RivalRequestBody {
  target_player_id?: string
  action?: 'add' | 'remove'
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)

  const body = (await readBody(event)) as RivalRequestBody
  const targetPlayerId = body.target_player_id
  if (!targetPlayerId || targetPlayerId === user.id) {
    throw createError({ statusCode: 400, statusMessage: 'valid target_player_id is required' })
  }

  if (body.action === 'remove') {
    await db
      .delete(schema.rivalRelationships)
      .where(
        and(
          eq(schema.rivalRelationships.ownerPlayerId, user.id),
          eq(schema.rivalRelationships.targetPlayerId, targetPlayerId),
          eq(schema.rivalRelationships.relationType, 'rival'),
        ),
      )
    return { removed: true }
  }

  await db
    .insert(schema.rivalRelationships)
    .values({
      ownerPlayerId: user.id,
      targetPlayerId,
      relationType: 'rival',
    })
    .onConflictDoNothing()
  return { added: true }
})
