import { desc, like } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  let request = db
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
    .$dynamic()

  if (search) {
    request = request.where(like(schema.charts.title, `%${escapeLikePattern(search)}%`))
  }

  return { charts: await request }
})

function escapeLikePattern(value: string): string {
  return value.replace(/[\\%_]/g, (match) => `\\${match}`)
}
