import { eq, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { IR_RANKING_MAX_OFFSET } from '../../../../../shared/constants/ir'
import { asRuleMode, normalizeGaugeName, requireHex } from '../../../../services/ir'
import { clampInteger } from '../../../../services/ir/common'
import { isPublicCourse, publicPlayerCondition } from '../../../../services/moderation/visibility'
import { resolveIrUser } from '../../../../utils/auth'
import {
  courseComboBreaks,
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
  const limit = clampInteger(query.limit, 100, 1, 200)
  const offset = clampInteger(query.offset, 0, 0, IR_RANKING_MAX_OFFSET)
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

  const ranked = rankCourseRows(
    await fetchCourseRankingRows(conditions),
    user?.id ?? null,
    limit,
    offset,
  )
  const visibleIds = new Set(ranked.entries.map(({ row }) => row.course_score_id))
  if (ranked.self) visibleIds.add(ranked.self.row.course_score_id)
  const details =
    visibleIds.size > 0
      ? await db
          .select({
            id: schema.courseScores.id,
            display_name: schema.profiles.displayName,
            client_name: schema.courseScores.clientName,
            client_version: schema.courseScores.clientVersion,
            arrange_options: schema.courseScores.arrangeOptions,
            judges: schema.courseScores.judges,
          })
          .from(schema.courseScores)
          .leftJoin(schema.profiles, eq(schema.profiles.id, schema.courseScores.playerId))
          .where(
            sql`${schema.courseScores.id} in (select value from json_each(${JSON.stringify([...visibleIds])}))`,
          )
      : []
  const detailsById = new Map(details.map((detail) => [detail.id, detail]))

  const toEntry = ({ rank, row }: RankedCourseRow) => {
    const detail = detailsById.get(row.course_score_id)
    return {
      rank,
      player: {
        id: row.player_id,
        display_name: detail?.display_name || 'Player',
      },
      score: {
        course_score_id: row.course_score_id,
        client_name: detail?.client_name ?? null,
        client_version: detail?.client_version ?? null,
        clear: row.clear_type,
        course_clear: row.course_clear,
        ex_score: row.ex_score,
        max_ex_score: row.max_ex_score,
        max_combo: row.max_combo,
        bp: row.bp,
        cb: courseComboBreaks(row.bp, detail?.judges),
        device_type: row.device_type,
        ln_policy: row.ln_policy,
        rule_mode: row.rule_mode,
        arrange_options: detail?.arrange_options ?? [],
        played_at: row.played_at,
        verification: row.verification,
      },
      relation: {
        is_self: row.player_id === user?.id,
        is_rival: false,
      },
    }
  }
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
        offset,
        total: ranked.total,
        has_more: offset + limit < ranked.total,
      },
    },
  }
})
