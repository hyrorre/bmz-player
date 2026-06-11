import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery } from 'h3'
import { requireHex } from '../../../../services/ir'
import type { Database } from '../../../../../shared/types/database.types'

/** コースランキング (global のみ)。EX score 降順、同点同順位。 */
export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')

  const query = getQuery(event)
  const gauge = typeof query.gauge === 'string' && query.gauge ? query.gauge : 'class'
  const lnPolicy = typeof query.ln_policy === 'string' && query.ln_policy ? query.ln_policy : 'AutoLn'
  const limit = Math.max(1, Math.min(200, Number(query.limit ?? 100) || 100))

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: rows, error } = await db
    .from('best_course_scores')
    .select(
      'player_id, course_score_id, ex_score, clear_type, clear_rank, course_clear, max_combo, bp, device_type, played_at, server_received_at, verification',
    )
    .eq('course_hash', courseHash)
    .eq('gauge', gauge)
    .eq('ln_policy', lnPolicy)
    .eq('scoring', 'bms_ex_score_v1')
    .order('ex_score', { ascending: false })
    .limit(limit)
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }

  const playerIds = [...new Set((rows ?? []).map((row) => row.player_id))]
  const { data: profiles } =
    playerIds.length > 0
      ? await db.from('profiles').select('id, display_name').in('id', playerIds)
      : { data: [] }
  const names = new Map((profiles ?? []).map((profile) => [profile.id, profile.display_name]))

  let previousEx: number | null = null
  let rank = 0
  const entries = (rows ?? []).map((row, index) => {
    if (previousEx !== row.ex_score) {
      rank = index + 1
      previousEx = row.ex_score
    }
    return {
      rank,
      player: { id: row.player_id, display_name: names.get(row.player_id) || 'Player' },
      score: {
        course_score_id: row.course_score_id,
        clear: row.clear_type,
        course_clear: row.course_clear,
        ex_score: row.ex_score,
        max_combo: row.max_combo,
        bp: row.bp,
        device_type: row.device_type,
        played_at: row.played_at,
        verification: row.verification,
      },
    }
  })

  return {
    course: { course_hash: courseHash },
    rule: { gauge, ln_policy: lnPolicy, scoring: 'bms_ex_score_v1' },
    ranking: { scope: 'global', sort: 'ex_score_desc', entries },
  }
})
