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
    .orderBy(desc(schema.charts.updatedAt))
    .limit(limit)
    .offset(offset)
    .$dynamic()
  let countRequest = db
    .select({ total: sql<number>`count(*)` })
    .from(schema.charts)
    .$dynamic()

  if (search) {
    const pattern = `%${escapeLikePattern(search)}%`
    const condition = sql`${schema.charts.title} like ${pattern} escape '\\'`
    listRequest = listRequest.where(condition)
    countRequest = countRequest.where(condition)
  }

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
