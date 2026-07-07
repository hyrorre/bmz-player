import { desc, inArray, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  let request = db
    .select({
      id: schema.profiles.id,
      display_name: schema.profiles.displayName,
      bio: schema.profiles.bio,
      updated_at: schema.profiles.updatedAt,
    })
    .from(schema.profiles)
    .orderBy(desc(schema.profiles.updatedAt))
    .limit(limit)
    .offset(offset)
    .$dynamic()
  let countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(schema.profiles)
    .$dynamic()

  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    const condition = sql`${schema.profiles.displayName} like ${pattern} escape '\\' or ${schema.profiles.id} like ${pattern} escape '\\'`
    request = request.where(condition)
    countRequest = countRequest.where(condition)
  }

  const [profiles, countRows] = await Promise.all([request, countRequest])
  const total = Number(countRows[0]?.total ?? 0)
  const playerIds = profiles.map((profile) => profile.id)

  const bestScoreRows =
    playerIds.length > 0
      ? await db
          .select({
            player_id: schema.bestScores.playerId,
            best_score_count: sql<number>`count(*)`,
            last_score_at: sql<number | null>`max(${schema.bestScores.serverReceivedAt})`,
          })
          .from(schema.bestScores)
          .where(inArray(schema.bestScores.playerId, playerIds))
          .groupBy(schema.bestScores.playerId)
      : []

  const bestCourseRows =
    playerIds.length > 0
      ? await db
          .select({
            player_id: schema.bestCourseScores.playerId,
            best_course_score_count: sql<number>`count(*)`,
            last_course_score_at: sql<
              number | null
            >`max(${schema.bestCourseScores.serverReceivedAt})`,
          })
          .from(schema.bestCourseScores)
          .where(inArray(schema.bestCourseScores.playerId, playerIds))
          .groupBy(schema.bestCourseScores.playerId)
      : []

  const bestScoreMap = new Map(bestScoreRows.map((row) => [row.player_id, row]))
  const bestCourseMap = new Map(bestCourseRows.map((row) => [row.player_id, row]))

  return {
    players: profiles
      .map((profile) => {
        const score = bestScoreMap.get(profile.id)
        const course = bestCourseMap.get(profile.id)
        const lastScoreAt = latestTimestamp(score?.last_score_at, course?.last_course_score_at)

        return {
          ...profile,
          best_score_count: Number(score?.best_score_count ?? 0),
          best_course_score_count: Number(course?.best_course_score_count ?? 0),
          last_score_at: lastScoreAt == null ? null : new Date(lastScoreAt).toISOString(),
        }
      })
      .sort((a, b) => {
        const aTime = toTimestamp(a.last_score_at ?? a.updated_at)
        const bTime = toTimestamp(b.last_score_at ?? b.updated_at)
        return bTime - aTime
      }),
    pagination: {
      limit,
      offset,
      total,
      has_more: offset + limit < total,
    },
  }
})

function escapeLikePattern(value: string): string {
  return value.replace(/[\\%_]/g, (match) => `\\${match}`)
}

function latestTimestamp(
  left: Date | string | number | null | undefined,
  right: Date | string | number | null | undefined,
): number | null {
  if (!left) {
    return right == null ? null : toTimestamp(right)
  }
  if (!right) {
    return toTimestamp(left)
  }
  return Math.max(toTimestamp(left), toTimestamp(right))
}

function toTimestamp(value: Date | string | number): number {
  if (value instanceof Date) {
    return value.getTime()
  }
  if (typeof value === 'number') {
    return value
  }
  return new Date(value).getTime()
}
