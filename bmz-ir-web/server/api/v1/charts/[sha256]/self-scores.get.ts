import { and, desc, eq, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import type { IrScoreHistoryResult } from '../../../../../shared/types/ir'
import {
  arrangeOptionsFromPlayOptions,
  parseRankingQuery,
  requireHex,
} from '../../../../services/ir'
import { requireIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event): Promise<IrScoreHistoryResult> => {
  const sha256 = getRouterParam(event, 'sha256')
  if (!sha256) {
    throw createError({ statusCode: 400, statusMessage: 'chart sha256 is required' })
  }
  requireHex(sha256, 64, 'sha256')

  const user = await requireIrUser(event)
  const query = parseRankingQuery(getQuery(event))
  const conditions = [
    eq(schema.scores.playerId, user.id),
    eq(schema.scores.chartSha256, sha256),
    eq(schema.scores.scoring, query.scoring),
    eq(schema.scores.doubleOption, query.doubleOption),
    eq(schema.scores.accepted, true),
  ]
  if (query.lnPolicy) {
    conditions.push(eq(schema.scores.lnPolicy, query.lnPolicy))
  }
  if (query.ruleMode) {
    conditions.push(eq(schema.scores.ruleMode, query.ruleMode))
  }

  const [countRows, rows] = await Promise.all([
    db
      .select({ total: sql<number>`count(*)` })
      .from(schema.scores)
      .where(and(...conditions)),
    db
      .select({
        score_id: schema.scores.id,
        clear: schema.scores.clearType,
        ex_score: schema.scores.exScore,
        max_combo: schema.scores.maxCombo,
        min_bp: schema.scores.minBp,
        min_cb: schema.scores.minCb,
        bp: schema.scores.bp,
        cb: schema.scores.cb,
        gauge: schema.scores.gauge,
        ln_policy: schema.scores.lnPolicy,
        double_option: schema.scores.doubleOption,
        rule_mode: schema.scores.ruleMode,
        device_type: schema.scores.deviceType,
        played_at: schema.scores.playedAt,
        server_received_at: schema.scores.serverReceivedAt,
        verification: schema.scores.verification,
        play_options: schema.scores.playOptions,
      })
      .from(schema.scores)
      .where(and(...conditions))
      .orderBy(desc(schema.scores.serverReceivedAt))
      .limit(query.limit)
      .offset(query.offset),
  ])
  const total = Number(countRows[0]?.total ?? 0)

  return {
    chart: { sha256 },
    rule: {
      scoring: query.scoring,
      ln_policy: query.lnPolicy,
      double_option: query.doubleOption,
      rule_mode: query.ruleMode,
    },
    scores: rows.map((row) => {
      const { play_options: playOptions, ...score } = row
      return {
        ...score,
        ...arrangeOptionsFromPlayOptions(playOptions),
        ln_policy: score.ln_policy as IrScoreHistoryResult['scores'][number]['ln_policy'],
        double_option:
          score.double_option as IrScoreHistoryResult['scores'][number]['double_option'],
        rule_mode: score.rule_mode as IrScoreHistoryResult['scores'][number]['rule_mode'],
        device_type: score.device_type as IrScoreHistoryResult['scores'][number]['device_type'],
        played_at: score.played_at?.toISOString() ?? null,
        server_received_at: score.server_received_at.toISOString(),
      }
    }),
    pagination: {
      limit: query.limit,
      offset: query.offset,
      total,
      has_more: query.offset + query.limit < total,
    },
  }
})
