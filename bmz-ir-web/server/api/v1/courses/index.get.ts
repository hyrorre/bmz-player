import { desc, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  let listRequest = db
    .select({
      course_hash: schema.irCourses.courseHash,
      title: schema.irCourses.title,
      kind: schema.irCourses.kind,
      chart_count: schema.irCourses.chartCount,
      updated_at: schema.irCourses.updatedAt,
    })
    .from(schema.irCourses)
    .orderBy(desc(schema.irCourses.updatedAt))
    .limit(limit)
    .offset(offset)
    .$dynamic()
  let countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(schema.irCourses)
    .$dynamic()

  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    const condition = sql`${schema.irCourses.title} like ${pattern} escape '\\' or ${schema.irCourses.courseHash} like ${pattern} escape '\\'`
    listRequest = listRequest.where(condition)
    countRequest = countRequest.where(condition)
  }

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
