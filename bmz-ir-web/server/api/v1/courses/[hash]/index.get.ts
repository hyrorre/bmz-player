import { serverSupabaseServiceRole } from '#supabase/server'
import { requireHex } from '../../../../services/ir'
import type { Database } from '../../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: course, error } = await db
    .from('ir_courses')
    .select('*')
    .eq('course_hash', courseHash)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!course) {
    throw createError({ statusCode: 404, statusMessage: 'Course not found' })
  }

  const { count } = await db
    .from('course_scores')
    .select('id', { count: 'exact', head: true })
    .eq('course_hash', courseHash)
    .eq('accepted', true)

  return { course, stats: { play_count: count ?? 0 } }
})
