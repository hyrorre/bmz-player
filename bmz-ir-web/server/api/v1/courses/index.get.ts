import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery } from 'h3'
import type { Database } from '../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))

  const db = serverSupabaseServiceRole<Database>(event)
  const { data, error } = await db
    .from('ir_courses')
    .select('course_hash, title, kind, chart_count, updated_at')
    .order('updated_at', { ascending: false })
    .limit(limit)
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return { courses: data ?? [] }
})
