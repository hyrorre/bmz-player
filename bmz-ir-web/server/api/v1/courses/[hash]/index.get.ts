import { and, eq, inArray, sql } from 'drizzle-orm'
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

  const courseCharts =
    course.charts.length > 0
      ? await db
          .select({
            sha256: schema.charts.sha256,
            title: schema.charts.title,
            subtitle: schema.charts.subtitle,
            artist: schema.charts.artist,
            mode: schema.charts.mode,
            level: schema.charts.level,
            difficulty: schema.charts.difficulty,
          })
          .from(schema.charts)
          .where(inArray(schema.charts.sha256, course.charts))
      : []
  const chartMap = new Map(courseCharts.map((chart) => [chart.sha256, chart]))

  return {
    course: courseToResponse(course, chartMap),
    stats: { play_count: Number(stats[0]?.play_count ?? 0) },
  }
})

function courseToResponse(
  course: typeof schema.irCourses.$inferSelect,
  chartMap: Map<string, CourseChartResponse>,
) {
  return {
    course_hash: course.courseHash,
    title: course.title,
    kind: course.kind,
    charts: course.charts.map((sha256) => {
      const chart = chartMap.get(sha256)
      return {
        sha256,
        title: chart?.title ?? '',
        subtitle: chart?.subtitle ?? null,
        artist: chart?.artist ?? null,
        mode: chart?.mode ?? null,
        level: chart?.level ?? null,
        difficulty: chart?.difficulty ?? null,
      }
    }),
    chart_count: course.chartCount,
    constraints: course.constraints,
    source_url: course.sourceUrl,
    created_at: course.createdAt,
    updated_at: course.updatedAt,
  }
}

interface CourseChartResponse {
  sha256: string
  title: string
  subtitle: string | null
  artist: string | null
  mode: string
  level: number | null
  difficulty: string | null
}
