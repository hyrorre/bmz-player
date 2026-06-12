import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const row = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      display_name: schema.profiles.displayName,
      bio: schema.profiles.bio,
    })
    .from(schema.users)
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .where(eq(schema.users.id, user.id))
    .limit(1)

  const profile = row[0]
  if (!profile) {
    throw createError({ statusCode: 404, statusMessage: 'Profile not found' })
  }

  return {
    player: {
      id: profile.id,
      email: profile.email,
      display_name: profile.display_name ?? '',
      bio: profile.bio ?? '',
    },
  }
})
