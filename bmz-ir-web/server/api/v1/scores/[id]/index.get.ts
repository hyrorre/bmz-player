import { serverSupabaseServiceRole } from '#supabase/server'
import type { Database } from '../../../../../shared/types/database.types'

/**
 * スコア詳細。ランキングに公開されている情報の単票ビュー。
 * 署名・リプレイの検証状態も返す。
 */
export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: score, error } = await db
    .from('scores')
    .select(
      'id, player_id, chart_sha256, clear_type, ex_score, max_combo, min_bp, min_cb, bp, cb, gauge, ln_policy, effective_ln_mode, scoring, judges, device_type, platform, client_name, client_version, played_at, server_received_at, verification, replay_hash',
    )
    .eq('id', scoreId)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!score) {
    throw createError({ statusCode: 404, statusMessage: 'Score not found' })
  }

  const [{ data: profile }, { data: chart }, { data: replay }] = await Promise.all([
    db.from('profiles').select('id, display_name').eq('id', score.player_id).maybeSingle(),
    db
      .from('charts')
      .select('sha256, title, subtitle, artist, mode, level, notes')
      .eq('sha256', score.chart_sha256)
      .maybeSingle(),
    db
      .from('replay_objects')
      .select('status, size_bytes, format')
      .eq('score_id', score.id)
      .maybeSingle(),
  ])

  return {
    score,
    player: profile ?? { id: score.player_id, display_name: 'Player' },
    chart,
    replay: replay ?? null,
  }
})
