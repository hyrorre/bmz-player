import { and, asc, desc, eq, sql, type SQL } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { readListSort } from '../../../../shared/utils/list_sort'
import {
  publicChartCondition,
  publicCourseCondition,
  publicPlayerCondition,
} from '../../../services/moderation/visibility'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''
  const { sort, order } = readListSort('players', query)
  const direction = order === 'asc' ? asc : desc
  const { users, profiles, scores, courseScores, bestScores, bestCourseScores } = schema

  const conditions: SQL[] = [publicPlayerCondition(profiles.id)]
  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    conditions.push(
      sql`(${profiles.displayName} like ${pattern} escape '\\' or ${profiles.id} like ${pattern} escape '\\')`,
    )
  }

  const chartSubmission = db
    .select({ received: scores.serverReceivedAt })
    .from(scores)
    .where(
      and(
        eq(scores.playerId, profiles.id),
        eq(scores.accepted, true),
        publicChartCondition(scores.chartSha256),
      ),
    )
    .orderBy(desc(scores.serverReceivedAt))
    .limit(1)
  const courseSubmission = db
    .select({ received: courseScores.serverReceivedAt })
    .from(courseScores)
    .where(
      and(
        eq(courseScores.playerId, profiles.id),
        eq(courseScores.accepted, true),
        publicCourseCondition(courseScores.courseHash),
      ),
    )
    .orderBy(desc(courseScores.serverReceivedAt))
    .limit(1)
  // SQLite's scalar max propagates NULL. Use a value below any server timestamp for missing submissions.
  const lastScoreAt =
    sql`nullif(max(coalesce((${chartSubmission}), -1), coalesce((${courseSubmission}), -1)), -1)`
      .mapWith((value: number) => new Date(value).toISOString())
      .as('last_score_at')

  const chartCount = db
    .select({ count: sql`count(*)` })
    .from(bestScores)
    .where(and(eq(bestScores.playerId, profiles.id), publicChartCondition(bestScores.chartSha256)))
  const courseCount = db
    .select({ count: sql`count(*)` })
    .from(bestCourseScores)
    .where(
      and(
        eq(bestCourseScores.playerId, profiles.id),
        publicCourseCondition(bestCourseScores.courseHash),
      ),
    )
  const bestScoreCount = sql`(${chartCount})`.mapWith(Number).as('best_score_count')
  const bestCourseScoreCount = sql`(${courseCount})`.mapWith(Number).as('best_course_score_count')

  const sortColumns = {
    last_score_at: lastScoreAt,
    display_name: sql`nullif(${profiles.displayName}, '') collate nocase`,
    created_at: sql`(select ${users.createdAt} from ${users} where ${users.id} = ${profiles.id})`,
    best_score_count: bestScoreCount,
    best_course_score_count: bestCourseScoreCount,
  }
  const sortColumn = sortColumns[sort]
  const orderBy = [sql`${direction(sortColumn)} nulls last`]
  if (sort === 'last_score_at') orderBy.push(desc(profiles.updatedAt))

  const request = db
    .select({
      id: profiles.id,
      display_name: profiles.displayName,
      bio: profiles.bio,
      updated_at: profiles.updatedAt,
      best_score_count: bestScoreCount,
      best_course_score_count: bestCourseScoreCount,
      last_score_at: lastScoreAt,
    })
    .from(profiles)
    .where(and(...conditions))
    .orderBy(...orderBy, profiles.id)
    .limit(limit)
    .offset(offset)
  const countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(profiles)
    .where(and(...conditions))

  const [players, countRows] = await Promise.all([request, countRequest])
  const total = Number(countRows[0]?.total ?? 0)
  return {
    players,
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
