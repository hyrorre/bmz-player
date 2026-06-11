import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery } from 'h3'
import type { Database } from '../../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const playerId = getRouterParam(event, 'id')
  if (!playerId) {
    throw createError({ statusCode: 400, statusMessage: 'player id is required' })
  }

  const query = getQuery(event)
  const limit = Math.max(1, Math.min(200, Number(query.limit ?? 50) || 50))

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: profile, error: profileError } = await db
    .from('profiles')
    .select('id, display_name, bio')
    .eq('id', playerId)
    .maybeSingle()
  if (profileError) {
    throw createError({ statusCode: 500, statusMessage: profileError.message })
  }
  if (!profile) {
    throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  }

  const { data: bests, error: bestsError } = await db
    .from('best_scores')
    .select(
      'score_id, chart_sha256, ex_score, clear_type, clear_rank, max_combo, min_bp, min_cb, device_type, gauge, ln_policy, scoring, played_at, server_received_at',
    )
    .eq('player_id', playerId)
    .order('server_received_at', { ascending: false })
    .limit(limit)
  if (bestsError) {
    throw createError({ statusCode: 500, statusMessage: bestsError.message })
  }

  const shaList = [...new Set((bests ?? []).map((row) => row.chart_sha256))]
  const { data: charts, error: chartsError } =
    shaList.length > 0
      ? await db.from('charts').select('sha256, title, artist, mode, level').in('sha256', shaList)
      : { data: [], error: null }
  if (chartsError) {
    throw createError({ statusCode: 500, statusMessage: chartsError.message })
  }
  const chartMap = new Map((charts ?? []).map((chart) => [chart.sha256, chart]))

  return {
    player: profile,
    best_scores: (bests ?? []).map((row) => ({
      ...row,
      chart: chartMap.get(row.chart_sha256) ?? null,
    })),
  }
})
