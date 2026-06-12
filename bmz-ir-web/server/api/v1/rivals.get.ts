import { desc, and, eq, inArray } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)

  const data = await db
    .select({
      target_player_id: schema.rivalRelationships.targetPlayerId,
      relation_type: schema.rivalRelationships.relationType,
      created_at: schema.rivalRelationships.createdAt,
    })
    .from(schema.rivalRelationships)
    .where(
      and(
        eq(schema.rivalRelationships.ownerPlayerId, user.id),
        eq(schema.rivalRelationships.relationType, 'rival'),
      ),
    )
    .orderBy(desc(schema.rivalRelationships.createdAt))

  const targetIds = data.map((row) => row.target_player_id)
  const profiles =
    targetIds.length > 0
      ? await db
          .select({
            id: schema.profiles.id,
            display_name: schema.profiles.displayName,
            bio: schema.profiles.bio,
          })
          .from(schema.profiles)
          .where(inArray(schema.profiles.id, targetIds))
      : []

  const profileMap = new Map(profiles.map((profile) => [profile.id, profile]))
  return {
    rivals: data.map((row) => ({
      player_id: row.target_player_id,
      relation_type: row.relation_type,
      created_at: row.created_at,
      profile: profileMap.get(row.target_player_id) ?? null,
    })),
  }
})
