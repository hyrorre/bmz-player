import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery } from 'h3'
import type { Database } from '../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const search = typeof query.q === 'string' ? query.q.trim() : ''

  const db = serverSupabaseServiceRole<Database>(event)
  let request = db
    .from('charts')
    .select('sha256, title, subtitle, genre, artist, mode, level, notes, updated_at')
    .order('updated_at', { ascending: false })
    .limit(limit)
  if (search) {
    request = request.ilike('title', `%${search}%`)
  }
  const { data, error } = await request
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return { charts: data ?? [] }
})
