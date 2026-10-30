import { eq, inArray } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { asRuleMode, normalizeGaugeName, requireHex } from '../../../../services/ir'
import { isPublicCourse, publicPlayerCondition } from '../../../../services/moderation/visibility'
import { resolveIrUser } from '../../../../utils/auth'
import {
  fetchCourseRankingRows,
  rankCourseRows,
  type RankedCourseRow,
} from '../../../../services/course_ranking'

/** コースランキング (global のみ)。EX score 降順、同点同順位。 */
export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')
  if (!(await isPublicCourse(courseHash))) {
    throw createError({ statusCode: 404, statusMessage: 'Course not found' })
  }
  const user = await resolveIrUser(event)

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
    // 順位付けの前に除外し、順位と total に BAN 済みプレイヤーを混ぜない。
    publicPlayerCondition(schema.courseScores.playerId),
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

  const ranked = rankCourseRows(await fetchCourseRankingRows(conditions), user?.id ?? null, limit)
  const visibleRows = ranked.self ? [...ranked.entries, ranked.self] : ranked.entries
  const playerIds = [...new Set(visibleRows.map(({ row }) => row.player_id))]
  const profiles =
    playerIds.length > 0
      ? await db
          .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
          .from(schema.profiles)
          .where(inArray(schema.profiles.id, playerIds))
      : []
  const names = new Map(profiles.map((profile) => [profile.id, profile.display_name]))

  const toEntry = ({ rank, row }: RankedCourseRow) => ({
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
    relation: {
      is_self: row.player_id === user?.id,
      is_rival: false,
    },
  })
  const entries = ranked.entries.map(toEntry)
  const selfEntry = ranked.self ? toEntry(ranked.self) : undefined

  return {
    course: { course_hash: courseHash },
    rule: { gauge, ln_policy: lnPolicy, rule_mode: ruleMode, scoring: 'bms_ex_score_v1' },
    ranking: {
      scope: 'global',
      sort: 'ex_score_desc',
      entries,
      self: selfEntry
        ? {
            rank: selfEntry.rank,
            score_id: selfEntry.score.course_score_id,
            included_in_entries: entries.some((entry) => entry.relation.is_self),
            entry: selfEntry,
          }
        : undefined,
      pagination: {
        limit,
        offset: 0,
        total: ranked.total,
        has_more: limit < ranked.total,
      },
    },
  }
})
