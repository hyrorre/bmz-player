import { and, desc, sql, type SQL } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { publicChartCondition } from '../../../services/moderation/visibility'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  const conditions: SQL[] = [publicChartCondition(schema.charts.sha256)]
  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    conditions.push(sql`${schema.charts.title} like ${pattern} escape '\\'`)
  }

  const listRequest = db
    .select({
      sha256: schema.charts.sha256,
      title: schema.charts.title,
      subtitle: schema.charts.subtitle,
      genre: schema.charts.genre,
      artist: schema.charts.artist,
      mode: schema.charts.mode,
      level: schema.charts.level,
      difficulty: schema.charts.difficulty,
      notes: schema.charts.notes,
      updated_at: schema.charts.updatedAt,
    })
    .from(schema.charts)
    .where(and(...conditions))
    .orderBy(desc(schema.charts.updatedAt))
    .limit(limit)
    .offset(offset)
  const countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(schema.charts)
    .where(and(...conditions))

  const [charts, countRows] = await Promise.all([listRequest, countRequest])
  const total = Number(countRows[0]?.total ?? 0)

  return {
    charts,
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
