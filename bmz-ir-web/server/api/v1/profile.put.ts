import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

interface ProfileBody {
  display_name?: string
  bio?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as ProfileBody
  const displayName = body.display_name?.trim() ?? ''
  const bio = body.bio?.trim() ?? ''

  if (!displayName) {
    throw createError({ statusCode: 400, statusMessage: 'display_name is required' })
  }
  if (displayName.length > 64) {
    throw createError({ statusCode: 400, statusMessage: 'display_name is too long' })
  }
  if (bio.length > 1000) {
    throw createError({ statusCode: 400, statusMessage: 'bio is too long' })
  }

  await db
    .insert(schema.profiles)
    .values({ id: user.id, displayName, bio, updatedAt: new Date() })
    .onConflictDoUpdate({
      target: schema.profiles.id,
      set: { displayName, bio, updatedAt: new Date() },
    })
  await replaceUserSession(event, {
    user: { id: user.id, email: user.email, displayName },
  })

  return { player: { id: user.id, email: user.email, display_name: displayName, bio } }
})
