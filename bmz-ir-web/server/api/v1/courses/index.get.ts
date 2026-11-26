import { and, asc, desc, sql, type SQL } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { readListSort } from '../../../../shared/utils/list_sort'
import { publicCourseCondition } from '../../../services/moderation/visibility'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''
  const { sort, order } = readListSort('courses', query)
  const direction = order === 'asc' ? asc : desc
  const coursesTable = schema.irCourses
  const sortColumns = {
    created_at: coursesTable.createdAt,
    title: sql`nullif(${coursesTable.title}, '') collate nocase`,
    chart_count: coursesTable.chartCount,
  }
  const sortColumn = sortColumns[sort]

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
    .orderBy(sql`${direction(sortColumn)} nulls last`, schema.irCourses.courseHash)
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
