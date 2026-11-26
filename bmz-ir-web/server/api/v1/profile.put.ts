import { eq } from 'drizzle-orm'
import { createError, readBody } from 'h3'
import { db, schema } from 'hub:db'
import {
  DAILY_BOUNDARY_MAX_MINUTES,
  DAILY_BOUNDARY_MIN_MINUTES,
} from '../../../shared/constants/ir'
import { isDailyBoundaryMinutes } from '../../services/daily_report_core'
import { requireIrUser } from '../../utils/auth'
import { requireDisplayName } from '../../utils/auth_input'

interface ProfileBody {
  display_name?: string
  bio?: string
  daily_boundary_minutes?: number
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = ((await readBody(event)) ?? {}) as ProfileBody
  const displayName = requireDisplayName(body.display_name)
  const bio = body.bio?.trim() ?? ''
  const requestedBoundary = body.daily_boundary_minutes

  if (bio.length > 1000) {
    throw createError({ statusCode: 400, statusMessage: 'bio is too long' })
  }
  if (requestedBoundary !== undefined && !isDailyBoundaryMinutes(requestedBoundary)) {
    throw createError({
      statusCode: 400,
      statusMessage: `daily_boundary_minutes must be an integer from ${DAILY_BOUNDARY_MIN_MINUTES} to ${DAILY_BOUNDARY_MAX_MINUTES}`,
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
