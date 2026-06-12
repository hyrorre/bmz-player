import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const profiles = await db
    .select({
      displayName: schema.profiles.displayName,
      bio: schema.profiles.bio,
    })
    .from(schema.profiles)
    .where(eq(schema.profiles.id, user.id))
    .limit(1)
  const profile = profiles[0] ?? null

  return {
    player: {
      id: user.id,
      email: user.email ?? null,
      display_name: profile?.displayName ?? null,
      bio: profile?.bio ?? null,
    },
  }
})
