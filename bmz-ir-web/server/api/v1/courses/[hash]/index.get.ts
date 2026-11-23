import { and, eq, inArray, sql } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireHex } from '../../../../services/ir'
import { isPublicCourse, publicPlayerCondition } from '../../../../services/moderation/visibility'

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
  if (!course || !(await isPublicCourse(courseHash))) {
    throw createError({ statusCode: 404, statusMessage: 'Course not found' })
  }

  const stats = await db
    .select({ play_count: sql<number>`count(*)` })
    .from(schema.courseScores)
    .where(
      and(
        eq(schema.courseScores.courseHash, courseHash),
        eq(schema.courseScores.accepted, true),
        publicPlayerCondition(schema.courseScores.playerId),
      ),
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
            has_undefined_ln: schema.charts.hasUndefinedLn,
            has_defined_ln: schema.charts.hasDefinedLn,
            has_defined_cn: schema.charts.hasDefinedCn,
            has_defined_hcn: schema.charts.hasDefinedHcn,
            has_defined_hln: schema.charts.hasDefinedHln,
            has_ln: sql<boolean>`(
              ${schema.charts.hasLn} or ${schema.charts.hasCn} or ${schema.charts.hasHcn}
              or ${schema.charts.hasHln} or ${schema.charts.lnNotes} > 0
              or ${schema.charts.cnNotes} > 0 or ${schema.charts.hcnNotes} > 0
              or ${schema.charts.hlnNotes} > 0
            )`.mapWith(Boolean),
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
  const charts = [...chartMap.values()]
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
    ln_profile: {
      has_undefined_ln: charts.some((chart) => chart.has_undefined_ln),
      has_defined_ln: charts.some((chart) => chart.has_defined_ln),
      has_defined_cn: charts.some((chart) => chart.has_defined_cn),
      has_defined_hcn: charts.some((chart) => chart.has_defined_hcn),
      has_defined_hln: charts.some((chart) => chart.has_defined_hln),
      has_ln: charts.some((chart) => chart.has_ln),
    },
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
  has_undefined_ln: boolean
  has_defined_ln: boolean
  has_defined_cn: boolean
  has_defined_hcn: boolean
  has_defined_hln: boolean
  has_ln: boolean
}
