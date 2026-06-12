import { desc } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))

  const courses = await db
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

  return { courses }
})
