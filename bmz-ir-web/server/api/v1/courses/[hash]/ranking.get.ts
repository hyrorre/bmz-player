import { and, desc, eq, inArray } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { asRuleMode, normalizeGaugeName, requireHex } from '../../../../services/ir'

/** コースランキング (global のみ)。EX score 降順、同点同順位。 */
export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')

  const query = getQuery(event)
  const gauge = normalizeGaugeName(
    typeof query.gauge === 'string' && query.gauge ? query.gauge : 'Class',
  )
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy ? query.ln_policy : 'AutoLn'
  const ruleMode = asRuleMode(
    typeof query.rule_mode === 'string' && query.rule_mode ? query.rule_mode : 'Beatoraja',
  )
  const limit = Math.max(1, Math.min(200, Number(query.limit ?? 100) || 100))

  const rows = await db
    .select({
      player_id: schema.bestCourseScores.playerId,
      course_score_id: schema.bestCourseScores.courseScoreId,
      ex_score: schema.bestCourseScores.exScore,
      clear_type: schema.bestCourseScores.clearType,
      clear_rank: schema.bestCourseScores.clearRank,
      course_clear: schema.bestCourseScores.courseClear,
      max_combo: schema.bestCourseScores.maxCombo,
      bp: schema.bestCourseScores.bp,
      device_type: schema.bestCourseScores.deviceType,
      rule_mode: schema.bestCourseScores.ruleMode,
      played_at: schema.bestCourseScores.playedAt,
      server_received_at: schema.bestCourseScores.serverReceivedAt,
      verification: schema.bestCourseScores.verification,
    })
    .from(schema.bestCourseScores)
    .where(
      and(
        eq(schema.bestCourseScores.courseHash, courseHash),
        eq(schema.bestCourseScores.gauge, gauge),
        eq(schema.bestCourseScores.lnPolicy, lnPolicy),
        eq(schema.bestCourseScores.ruleMode, ruleMode),
        eq(schema.bestCourseScores.scoring, 'bms_ex_score_v1'),
      ),
    )
    .orderBy(desc(schema.bestCourseScores.exScore))
    .limit(limit)

  const playerIds = [...new Set(rows.map((row) => row.player_id))]
  const profiles =
    playerIds.length > 0
      ? await db
          .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
          .from(schema.profiles)
          .where(inArray(schema.profiles.id, playerIds))
      : []
  const names = new Map(profiles.map((profile) => [profile.id, profile.display_name]))

  let previousEx: number | null = null
  let rank = 0
  const entries = rows.map((row, index) => {
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
        rule_mode: row.rule_mode,
        played_at: row.played_at,
        verification: row.verification,
      },
    }
  })

  return {
    course: { course_hash: courseHash },
    rule: { gauge, ln_policy: lnPolicy, rule_mode: ruleMode, scoring: 'bms_ex_score_v1' },
    ranking: { scope: 'global', sort: 'ex_score_desc', entries },
  }
})
