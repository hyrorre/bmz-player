import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../utils/auth'

interface ProfileBody {
  display_name?: string
  bio?: string
  daily_boundary_minutes?: number
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as ProfileBody
  const displayName = body.display_name?.trim() ?? ''
  const bio = body.bio?.trim() ?? ''
  const requestedBoundary = body.daily_boundary_minutes

  if (!displayName) {
    throw createError({ statusCode: 400, statusMessage: 'display_name is required' })
  }
  if (displayName.length > 64) {
    throw createError({ statusCode: 400, statusMessage: 'display_name is too long' })
  }
  if (bio.length > 1000) {
    throw createError({ statusCode: 400, statusMessage: 'bio is too long' })
  }
  if (
    requestedBoundary !== undefined &&
    (!Number.isInteger(requestedBoundary) || requestedBoundary < 0 || requestedBoundary > 1439)
  ) {
    throw createError({
      statusCode: 400,
      statusMessage: 'daily_boundary_minutes must be an integer from 0 to 1439',
    })
  }

  const currentProfile = await db
    .select({ dailyBoundaryMinutes: schema.profiles.dailyBoundaryMinutes })
    .from(schema.profiles)
    .where(eq(schema.profiles.id, user.id))
    .limit(1)
  const dailyBoundaryMinutes = requestedBoundary ?? currentProfile[0]?.dailyBoundaryMinutes ?? 0

  await db
    .insert(schema.profiles)
    .values({ id: user.id, displayName, bio, dailyBoundaryMinutes, updatedAt: new Date() })
    .onConflictDoUpdate({
      target: schema.profiles.id,
      set: { displayName, bio, dailyBoundaryMinutes, updatedAt: new Date() },
    })
  await replaceUserSession(event, {
    user: { id: user.id, email: user.email, displayName },
  })

  return {
    player: {
      id: user.id,
      email: user.email,
      display_name: displayName,
      bio,
      daily_boundary_minutes: dailyBoundaryMinutes,
    },
  }
})
