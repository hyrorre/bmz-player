import { and, desc, sql, type SQL } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { publicCourseCondition } from '../../../services/moderation/visibility'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  const conditions: SQL[] = [publicCourseCondition(schema.irCourses.courseHash)]
  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    conditions.push(
      sql`(${schema.irCourses.title} like ${pattern} escape '\\' or ${schema.irCourses.courseHash} like ${pattern} escape '\\')`,
    )
  }

  const listRequest = db
    .select({
      course_hash: schema.irCourses.courseHash,
      title: schema.irCourses.title,
      kind: schema.irCourses.kind,
      chart_count: schema.irCourses.chartCount,
      updated_at: schema.irCourses.updatedAt,
    })
    .from(schema.irCourses)
    .where(and(...conditions))
    .orderBy(desc(schema.irCourses.updatedAt))
    .limit(limit)
    .offset(offset)
  const countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(schema.irCourses)
    .where(and(...conditions))

  const [courses, countRows] = await Promise.all([listRequest, countRequest])
  const total = Number(countRows[0]?.total ?? 0)

  return {
    courses,
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
