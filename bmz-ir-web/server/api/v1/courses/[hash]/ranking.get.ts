import { and, asc, desc, eq, inArray } from 'drizzle-orm'
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
  const gauge =
    typeof query.gauge === 'string' && query.gauge && query.gauge !== 'ALL'
      ? normalizeGaugeName(query.gauge)
      : null
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy && query.ln_policy !== 'ALL'
      ? query.ln_policy
      : null
  const ruleMode =
    typeof query.rule_mode === 'string' && query.rule_mode && query.rule_mode !== 'ALL'
      ? asRuleMode(query.rule_mode)
      : null
  const limit = Math.max(1, Math.min(200, Number(query.limit ?? 100) || 100))
  const conditions = [
    eq(schema.courseScores.courseHash, courseHash),
    eq(schema.courseScores.accepted, true),
    eq(schema.courseScores.scoring, 'bms_ex_score_v1'),
  ]
  if (gauge) {
    conditions.push(eq(schema.courseScores.gauge, gauge))
  }
  if (lnPolicy) {
    conditions.push(eq(schema.courseScores.lnPolicy, lnPolicy))
  }
  if (ruleMode) {
    conditions.push(eq(schema.courseScores.ruleMode, ruleMode))
  }

  const rows = await db
    .select({
      player_id: schema.courseScores.playerId,
      course_score_id: schema.courseScores.id,
      ex_score: schema.courseScores.exScore,
      clear_type: schema.courseScores.clearType,
      clear_rank: schema.courseScores.clearRank,
      course_clear: schema.courseScores.courseClear,
      max_combo: schema.courseScores.maxCombo,
      bp: schema.courseScores.bp,
      device_type: schema.courseScores.deviceType,
      rule_mode: schema.courseScores.ruleMode,
      played_at: schema.courseScores.playedAt,
      server_received_at: schema.courseScores.serverReceivedAt,
      verification: schema.courseScores.verification,
    })
    .from(schema.courseScores)
    .where(and(...conditions))
    .orderBy(
      desc(schema.courseScores.exScore),
      desc(schema.courseScores.clearRank),
      asc(schema.courseScores.bp),
      desc(schema.courseScores.maxCombo),
      desc(schema.courseScores.serverReceivedAt),
    )
    .limit(Math.min(1000, limit * 10))

  const bestRowsByPlayer = new Map<string, (typeof rows)[number]>()
  for (const row of rows) {
    if (!bestRowsByPlayer.has(row.player_id)) {
      bestRowsByPlayer.set(row.player_id, row)
    }
    if (bestRowsByPlayer.size >= limit) {
      break
    }
  }
  const bestRows = [...bestRowsByPlayer.values()]
  const playerIds = [...new Set(bestRows.map((row) => row.player_id))]
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
  const entries = bestRows.map((row, index) => {
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
