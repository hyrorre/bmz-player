import { and, desc, eq, lt, or, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import type {
  IrDeviceType,
  IrDoubleOption,
  IrOwnScoreHistoryResult,
  IrRuleMode,
  LnScorePolicy,
} from '../../../../shared/types/ir'
import { arrangeOptionsFromPlayOptions } from '../../../services/ir'
import { requireIrUser } from '../../../utils/auth'
import {
  optionalHexQuery,
  ownScoreHistoryCursorFromQuery,
  ownScoreHistoryPage,
} from '../../../utils/score_history_query'

export default defineEventHandler(async (event): Promise<IrOwnScoreHistoryResult> => {
  const user = await requireIrUser(event)
  const query = getQuery(event)
  const limit = clampInteger(query.limit, 100, 1, 500)
  const offset = clampInteger(query.offset, 0, 0, 100_000)
  const cursor = ownScoreHistoryCursorFromQuery(query.cursor_received_at_ms, query.cursor_score_id)
  const scoring = String(query.scoring ?? 'bms_ex_score_v1')
  if (scoring !== 'bms_ex_score_v1') {
    throw createError({ statusCode: 400, statusMessage: 'unsupported scoring' })
  }

  const conditions = [
    eq(schema.scores.playerId, user.id),
    eq(schema.scores.scoring, scoring),
    eq(schema.scores.accepted, true),
  ]
  const chartSha256 = optionalHexQuery(query.chart_sha256, 64, 'chart_sha256')
  if (chartSha256) {
    conditions.push(eq(schema.scores.chartSha256, chartSha256))
  }
  if (typeof query.ln_policy === 'string' && query.ln_policy) {
    conditions.push(eq(schema.scores.lnPolicy, query.ln_policy))
  }
  if (typeof query.double_option === 'string' && query.double_option) {
    conditions.push(eq(schema.scores.doubleOption, asDoubleOption(query.double_option)))
  }
  if (typeof query.rule_mode === 'string' && query.rule_mode && query.rule_mode !== 'ALL') {
    conditions.push(eq(schema.scores.ruleMode, asRuleMode(query.rule_mode)))
  }
  const pageConditions = [...conditions]
  if (cursor) {
    const receivedAt = new Date(cursor.server_received_at_ms)
    pageConditions.push(
      or(
        lt(schema.scores.serverReceivedAt, receivedAt),
        and(eq(schema.scores.serverReceivedAt, receivedAt), lt(schema.scores.id, cursor.score_id)),
      )!,
    )
  }

  const [countRows, rows] = await Promise.all([
    db
      .select({ total: sql<number>`count(*)` })
      .from(schema.scores)
      .where(and(...conditions)),
    db
      .select({
        score_id: schema.scores.id,
        chart_sha256: schema.scores.chartSha256,
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
        applied_double_option: schema.scores.appliedDoubleOption,
        source_kind: schema.scores.sourceKind,
        rule_mode: schema.scores.ruleMode,
        judges: schema.scores.judges,
        notes: schema.scores.notes,
        pass_notes: schema.scores.passNotes,
        duration_ms: schema.scores.durationMs,
        device_type: schema.scores.deviceType,
        played_at: schema.scores.playedAt,
        server_received_at: schema.scores.serverReceivedAt,
        verification: schema.scores.verification,
        replay_hash: schema.scores.replayHash,
        play_options: schema.scores.playOptions,
      })
      .from(schema.scores)
      .where(and(...pageConditions))
      .orderBy(desc(schema.scores.serverReceivedAt), desc(schema.scores.id))
      .limit(limit + 1)
      .offset(cursor ? 0 : offset),
  ])
  const total = Number(countRows[0]?.total ?? 0)
  const page = ownScoreHistoryPage(rows, limit)

  return {
    scores: page.rows.map((row) => {
      const { play_options: playOptions, ...score } = row
      return {
        ...score,
        ...arrangeOptionsFromPlayOptions(playOptions),
        ...scoreOptionMetadata(playOptions),
        ln_policy: score.ln_policy as LnScorePolicy,
        double_option: score.double_option as IrDoubleOption,
        rule_mode: score.rule_mode as IrRuleMode,
        judges: score.judges as IrOwnScoreHistoryResult['scores'][number]['judges'],
        device_type: score.device_type as IrDeviceType,
        played_at: score.played_at ? unixSeconds(score.played_at) : null,
        server_received_at: unixSeconds(score.server_received_at),
        replay_hash: score.replay_hash ?? undefined,
      }
    }),
    pagination: {
      limit,
      offset,
      total,
      has_more: page.hasMore,
      next_cursor: page.nextCursor,
    },
  }
})

function clampInteger(value: unknown, defaultValue: number, min: number, max: number): number {
  const parsed = Number(value ?? defaultValue)
  if (!Number.isFinite(parsed)) {
    return defaultValue
  }
  return Math.min(max, Math.max(min, Math.trunc(parsed)))
}

function unixSeconds(value: Date): number {
  return Math.floor(value.getTime() / 1000)
}

function scoreOptionMetadata(playOptions: Record<string, unknown> | null | undefined): {
  random_seed?: number | string
  assist_mask?: number
  seed_scheme?: string
} {
  const metadata: { random_seed?: number | string; assist_mask?: number; seed_scheme?: string } = {}
  const randomSeed = playOptions?.random_seed
  if (typeof randomSeed === 'number' && Number.isSafeInteger(randomSeed)) {
    metadata.random_seed = randomSeed
  } else if (typeof randomSeed === 'string' && /^-?\d+$/.test(randomSeed)) {
    try {
      const parsed = BigInt(randomSeed)
      if (parsed >= -(1n << 63n) && parsed <= (1n << 63n) - 1n) {
        metadata.random_seed = randomSeed
      }
    } catch {
      // Invalid legacy metadata is omitted so one row cannot invalidate the page.
    }
  }
  const assistMask = playOptions?.assist_mask
  if (
    typeof assistMask === 'number' &&
    Number.isInteger(assistMask) &&
    assistMask >= 0 &&
    assistMask <= 0xffff_ffff
  ) {
    metadata.assist_mask = assistMask
  }
  if (typeof playOptions?.seed_scheme === 'string' && playOptions.seed_scheme.trim()) {
    metadata.seed_scheme = playOptions.seed_scheme
  }
  return metadata
}

function asDoubleOption(value: string): IrDoubleOption {
  if (value === 'off' || value === 'battle' || value === 'battle_auto_scratch') {
    return value
  }
  throw createError({ statusCode: 400, statusMessage: 'double_option is invalid' })
}

function asRuleMode(value: string): IrRuleMode {
  if (value === 'Beatoraja' || value === 'Lr2Oraja' || value === 'Dx') {
    return value
  }
  throw createError({ statusCode: 400, statusMessage: 'rule_mode is invalid' })
}
