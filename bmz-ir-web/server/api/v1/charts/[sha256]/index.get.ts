import { serverSupabaseServiceRole, serverSupabaseUser } from '#supabase/server'
import type { Database } from '../../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const sha256 = getRouterParam(event, 'sha256')
  if (!sha256) {
    throw createError({ statusCode: 400, statusMessage: 'chart sha256 is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const user = await serverSupabaseUser(event)
  const { data: chart, error } = await db.from('charts').select('*').eq('sha256', sha256).maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!chart) {
    throw createError({ statusCode: 404, statusMessage: 'Chart not found' })
  }

  const globalStats = await scoreStats(db, sha256)
  const selfStats = user ? await scoreStats(db, sha256, user.id) : null

  return {
    chart,
    stats: {
      global: globalStats,
      self: selfStats,
    },
  }
})

async function scoreStats(db: ReturnType<typeof serverSupabaseServiceRole<Database>>, sha256: string, playerId?: string) {
  let query = db
    .from('scores')
    .select('clear_rank', { count: 'exact', head: false })
    .eq('chart_sha256', sha256)
    .eq('accepted', true)
  if (playerId) {
    query = query.eq('player_id', playerId)
  }
  const { data, count, error } = await query
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return {
    play_count: count ?? 0,
    clear_count: (data ?? []).filter((score) => score.clear_rank > 1).length,
  }
}
