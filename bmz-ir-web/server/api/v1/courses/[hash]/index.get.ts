import { and, eq, sql } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireHex } from '../../../../services/ir'

export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')

  const courses = await db
    .select()
    .from(schema.irCourses)
    .where(eq(schema.irCourses.courseHash, courseHash))
    .limit(1)
  const course = courses[0]
  if (!course) {
    throw createError({ statusCode: 404, statusMessage: 'Course not found' })
  }

  const stats = await db
    .select({ play_count: sql<number>`count(*)` })
    .from(schema.courseScores)
    .where(
      and(eq(schema.courseScores.courseHash, courseHash), eq(schema.courseScores.accepted, true)),
    )

  return {
    course: courseToResponse(course),
    stats: { play_count: Number(stats[0]?.play_count ?? 0) },
  }
})

function courseToResponse(course: typeof schema.irCourses.$inferSelect) {
  return {
    course_hash: course.courseHash,
    title: course.title,
    kind: course.kind,
    charts: course.charts,
    chart_count: course.chartCount,
    constraints: course.constraints,
    source_url: course.sourceUrl,
    created_at: course.createdAt,
    updated_at: course.updatedAt,
  }
}
