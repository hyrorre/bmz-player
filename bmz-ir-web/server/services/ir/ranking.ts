import { and, desc, eq, inArray, isNull, or } from 'drizzle-orm'
import type { BatchItem } from 'drizzle-orm/batch'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../../utils/db_errors'
import { publicChartCondition, publicPlayerCondition } from '../moderation/visibility'
import type {
  IrAppliedDoubleOption,
  IrChartLnProfile,
  IrDeviceType,
  IrDoubleOption,
  IrJudgeCounts,
  IrJudges,
  IrRanking,
  IrRankingEntry,
  IrRankingScope,
  IrRuleMode,
  IrScoreSubmission,
  IrScoreSourceKind,
  IrSubmitResponse,
  IrVerificationStatus,
  LnScorePolicy,
} from '../../../shared/types/ir'

import {
  CLEAR_RANK,
  LOCAL_BACKFILL_SOURCE,
  type BestScoreCandidate,
  type BestScoreKey,
  type BestScoreRow,
  IrBackfillCleanupError,
  IrEvidenceValidationError,
  IrScoreNotFoundError,
  type IrRequestUser,
  type LocalBackfillDeleteResult,
  type RankingQuery,
  type ScoreAttestationPayload,
  type ScoreHistoryRankingRow,
  arrangeOptionsFromPlayOptions,
  bestCandidateWins,
  isRecord,
  judgeTotal,
  nonEmptyString,
  normalizeDoubleOption,
  normalizeGaugeName,
  playedAtDate,
  requireHex,
  scoreSubmissionMetadata,
} from './common'

export async function getRanking(
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
): Promise<IrRanking> {
  return (await buildRanking(user, sha256, query)).data
}

export async function getRankingWithPreviousRank(
  user: IrRequestUser,
  sha256: string,
  query: RankingQuery,
  previousBestExScore: number | null,
): Promise<{ data: IrRanking; previousRank: number }> {
  const result = await buildRanking(user, sha256, query)
  return {
    data: result.data,
    previousRank: previousRankFromEntries(result.ranked, user.id, previousBestExScore),
  }
}

async function buildRanking(
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
): Promise<{ data: IrRanking; ranked: IrRankingEntry[] }> {
  requireHex(sha256, 64, 'sha256')
  const bestRows = await fetchRankingBestRows(sha256, query)
  const rivalIds = user ? await getRivalIds(user.id) : new Set<string>()
  const rankingRows = dedupeBestRowsByPlayer(bestRows)
  const playerIds = [...new Set(rankingRows.map((row) => row.player_id))]
  const names = await getPlayerNames(playerIds)
  const ranked = rankRows(rankingRows, user?.id ?? null, rivalIds, names)
  const scoped = applyScope(ranked, query.scope, user?.id ?? null, rivalIds)
  const entries = scoped.slice(query.offset, query.offset + query.limit).map((entry, index) => ({
    ...entry,
    scope_rank: query.offset + index + 1,
  }))
  const selfEntry = ranked.find((entry) => entry.relation.is_self)

  return {
    data: {
      chart: { sha256 },
      rule: {
        scoring: query.scoring,
        ln_policy: query.lnPolicy,
        effective_ln_mode: query.lnPolicy
          ? rankingRows.find((row) => row.ln_policy === query.lnPolicy)?.effective_ln_mode
          : undefined,
        double_option: query.doubleOption,
        rule_mode: query.ruleMode,
      },
      ranking: {
        scope: query.scope,
        sort: 'ex_score_desc',
        // 全プレイヤー中のクリア率 (%)。NoPlay/Failed を除いた割合。
        clear_rate:
          rankingRows.length > 0
            ? Math.round(
                (rankingRows.filter((row) => row.clear_rank > 1).length / rankingRows.length) * 100,
              )
            : null,
        entries,
        self: selfEntry
          ? {
              rank: selfEntry.rank,
              score_id: selfEntry.score.score_id,
              included_in_entries: entries.some(
                (entry) => entry.score.score_id === selfEntry.score.score_id,
              ),
              entry: selfEntry,
            }
          : undefined,
        pagination: {
          limit: query.limit,
          offset: query.offset,
          total: scoped.length,
          has_more: query.offset + query.limit < scoped.length,
        },
      },
    },
    ranked,
  }
}

export function previousRankFromEntries(
  entries: IrRankingEntry[],
  selfId: string,
  previousBestExScore: number | null,
): number {
  if (previousBestExScore === null) {
    return 0
  }
  return (
    entries.filter(
      (entry) => entry.player.id !== selfId && entry.score.ex_score > previousBestExScore,
    ).length + 1
  )
}

export async function fetchRankingBestRows(
  sha256: string,
  query: RankingQuery,
): Promise<BestScoreRow[]> {
  const conditions = [
    eq(schema.bestScores.chartSha256, sha256),
    eq(schema.bestScores.scoring, query.scoring),
    eq(schema.bestScores.doubleOption, query.doubleOption),
    // BAN・非公開譜面は順位付けの前に除外し、順位・件数・クリア率に混ぜない。
    publicPlayerCondition(schema.bestScores.playerId),
    publicChartCondition(schema.bestScores.chartSha256),
  ]
  if (query.lnPolicy) {
    conditions.push(eq(schema.bestScores.lnPolicy, query.lnPolicy))
  }
  if (query.ruleMode) {
    conditions.push(eq(schema.bestScores.ruleMode, query.ruleMode))
  }

  const rows = await db
    .select({
      player_id: schema.bestScores.playerId,
      chart_sha256: schema.bestScores.chartSha256,
      score_id: schema.bestScores.scoreId,
      best_ex_score_id: schema.bestScores.bestExScoreId,
      best_clear_score_id: schema.bestScores.bestClearScoreId,
      best_max_combo_score_id: schema.bestScores.bestMaxComboScoreId,
      best_min_bp_score_id: schema.bestScores.bestMinBpScoreId,
      best_min_cb_score_id: schema.bestScores.bestMinCbScoreId,
      ex_score: schema.bestScores.exScore,
      clear_type: schema.bestScores.clearType,
      clear_rank: schema.bestScores.clearRank,
      max_combo: schema.bestScores.maxCombo,
      min_bp: schema.bestScores.minBp,
      min_cb: schema.bestScores.minCb,
      device_type: schema.bestScores.deviceType,
      gauge: schema.bestScores.gauge,
      ln_policy: schema.bestScores.lnPolicy,
      effective_ln_mode: schema.bestScores.effectiveLnMode,
      double_option: schema.bestScores.doubleOption,
      rule_mode: schema.bestScores.ruleMode,
      scoring: schema.bestScores.scoring,
      played_at: schema.bestScores.playedAt,
      server_received_at: schema.bestScores.serverReceivedAt,
      verification: schema.bestScores.verification,
    })
    .from(schema.bestScores)
    .where(and(...conditions))
    .orderBy(desc(schema.bestScores.exScore))

  const cachedRows = rows.map(rowToBestScoreRow)
  if (cachedRows.length > 0) {
    return enrichBestRowsWithPlayOptions(cachedRows)
  }
  return fetchRankingBestRowsFromHistory(sha256, query)
}

export async function enrichBestRowsWithPlayOptions(rows: BestScoreRow[]): Promise<BestScoreRow[]> {
  const scoreIds = [
    ...new Set(
      rows.flatMap((row) => [
        row.score_id,
        row.best_ex_score_id,
        row.best_clear_score_id,
        row.best_max_combo_score_id,
        row.best_min_bp_score_id,
        row.best_min_cb_score_id,
      ]),
    ),
  ]
  if (scoreIds.length === 0) {
    return rows
  }

  const scoreRows = await db
    .select({
      id: schema.scores.id,
      play_options: schema.scores.playOptions,
      judges: schema.scores.judges,
    })
    .from(schema.scores)
    .where(inArray(schema.scores.id, scoreIds))
  const scoreById = new Map(scoreRows.map((row) => [row.id, row]))
  return rows.map((row) => ({
    ...row,
    ...arrangeOptionsFromPlayOptions(scoreById.get(row.score_id)?.play_options),
    judges: rankingJudges(scoreById.get(row.best_ex_score_id)?.judges),
  }))
}

export async function fetchRankingBestRowsFromHistory(
  sha256: string,
  query: RankingQuery,
): Promise<BestScoreRow[]> {
  const conditions = [
    eq(schema.scores.chartSha256, sha256),
    eq(schema.scores.scoring, query.scoring),
    eq(schema.scores.doubleOption, query.doubleOption),
    eq(schema.scores.accepted, true),
    publicPlayerCondition(schema.scores.playerId),
    publicChartCondition(schema.scores.chartSha256),
  ]
  if (query.lnPolicy) {
    conditions.push(eq(schema.scores.lnPolicy, query.lnPolicy))
  }
  if (query.ruleMode) {
    conditions.push(eq(schema.scores.ruleMode, query.ruleMode))
  }

  const rows = await db
    .select({
      id: schema.scores.id,
      player_id: schema.scores.playerId,
      chart_sha256: schema.scores.chartSha256,
      ex_score: schema.scores.exScore,
      clear_type: schema.scores.clearType,
      clear_rank: schema.scores.clearRank,
      max_combo: schema.scores.maxCombo,
      min_bp: schema.scores.minBp,
      min_cb: schema.scores.minCb,
      device_type: schema.scores.deviceType,
      gauge: schema.scores.gauge,
      ln_policy: schema.scores.lnPolicy,
      effective_ln_mode: schema.scores.effectiveLnMode,
      double_option: schema.scores.doubleOption,
      rule_mode: schema.scores.ruleMode,
      scoring: schema.scores.scoring,
      played_at: schema.scores.playedAt,
      server_received_at: schema.scores.serverReceivedAt,
      verification: schema.scores.verification,
      judges: schema.scores.judges,
      play_options: schema.scores.playOptions,
    })
    .from(schema.scores)
    .where(and(...conditions))
    .orderBy(desc(schema.scores.exScore))

  return bestRowsFromHistory(
    rows.map((row) => ({ ...rowToBestScoreRow({ ...row, score_id: row.id }), id: row.id })),
  )
}

export function rowToBestScoreRow(row: {
  player_id: string
  chart_sha256: string
  score_id: string
  best_ex_score_id?: string | null
  best_clear_score_id?: string | null
  best_max_combo_score_id?: string | null
  best_min_bp_score_id?: string | null
  best_min_cb_score_id?: string | null
  ex_score: number
  clear_type: string
  clear_rank: number
  max_combo: number
  min_bp: number
  min_cb: number
  device_type: string
  gauge: string
  ln_policy: string
  effective_ln_mode: string
  double_option: string
  rule_mode: string
  played_at: Date | null
  server_received_at: Date
  verification: BestScoreRow['verification']
  judges?: Record<string, unknown> | null
  play_options?: Record<string, unknown> | null
}): BestScoreRow {
  const { play_options: playOptions, ...rowFields } = row
  const arrangeOptions = arrangeOptionsFromPlayOptions(playOptions)
  return {
    ...rowFields,
    ...arrangeOptions,
    best_ex_score_id: row.best_ex_score_id ?? row.score_id,
    best_clear_score_id: row.best_clear_score_id ?? row.score_id,
    best_max_combo_score_id: row.best_max_combo_score_id ?? row.score_id,
    best_min_bp_score_id: row.best_min_bp_score_id ?? row.score_id,
    best_min_cb_score_id: row.best_min_cb_score_id ?? row.score_id,
    scoring: 'bms_ex_score_v1',
    ln_policy: row.ln_policy as LnScorePolicy,
    effective_ln_mode: row.effective_ln_mode as 'ln' | 'cn' | 'hcn',
    double_option: row.double_option as IrDoubleOption,
    rule_mode: row.rule_mode as IrRuleMode,
    device_type: row.device_type as IrDeviceType,
    played_at: row.played_at?.toISOString() ?? null,
    judges: rankingJudges(row.judges),
  }
}

export function rankingJudgeCounts(value: unknown): IrJudgeCounts | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const keys = ['pgreat', 'great', 'good', 'bad', 'poor', 'empty_poor'] as const
  if (keys.some((key) => !Number.isInteger(value[key]) || Number(value[key]) < 0)) {
    return undefined
  }
  return {
    pgreat: Number(value.pgreat),
    great: Number(value.great),
    good: Number(value.good),
    bad: Number(value.bad),
    poor: Number(value.poor),
    empty_poor: Number(value.empty_poor),
  }
}

export function rankingJudges(value: unknown): IrJudges | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const fast = rankingJudgeCounts(value.fast)
  const slow = rankingJudgeCounts(value.slow)
  return fast && slow ? { fast, slow } : undefined
}

export function bestRowsFromHistory(rows: ScoreHistoryRankingRow[]): BestScoreRow[] {
  const bestByRule = new Map<string, BestScoreRow>()
  for (const row of rows) {
    const candidate = historyRowToBestRow(row)
    const key = bestRowKey(candidate)
    const current = bestByRule.get(key)
    if (current) {
      bestByRule.set(key, mergeBestRows(current, candidate))
    } else {
      bestByRule.set(key, candidate)
    }
  }
  return [...bestByRule.values()]
}

export function historyRowToBestRow(row: ScoreHistoryRankingRow): BestScoreRow {
  const { id, ...score } = row
  return { ...score, score_id: id }
}

export function bestRowKey(row: BestScoreRow): string {
  return [
    row.player_id,
    row.chart_sha256,
    row.ln_policy,
    row.scoring,
    row.double_option,
    row.rule_mode,
  ].join('\0')
}

export function bestRowWins(next: BestScoreRow, current: BestScoreRow): boolean {
  if (bestCandidateWins(next, current)) {
    return true
  }
  if (bestCandidateWins(current, next)) {
    return false
  }
  return (
    String(next.played_at ?? next.server_received_at).localeCompare(
      String(current.played_at ?? current.server_received_at),
    ) < 0
  )
}

export function dedupeBestRowsByPlayer(rows: BestScoreRow[]): BestScoreRow[] {
  const bestByPlayer = new Map<string, BestScoreRow>()
  for (const row of rows) {
    const current = bestByPlayer.get(row.player_id)
    bestByPlayer.set(row.player_id, current ? mergeBestRows(current, row) : row)
  }
  return [...bestByPlayer.values()]
}

export function mergeBestRows(current: BestScoreRow, next: BestScoreRow): BestScoreRow {
  const ranking = bestRowWins(next, current) ? next : current
  const clear = bestClearWins(next, current) ? next : current
  const combo = next.max_combo > current.max_combo ? next : current
  const bp = next.min_bp < current.min_bp ? next : current
  const cb = next.min_cb < current.min_cb ? next : current

  return {
    ...ranking,
    clear_type: clear.clear_type,
    clear_rank: clear.clear_rank,
    max_combo: combo.max_combo,
    min_bp: bp.min_bp,
    min_cb: cb.min_cb,
    best_ex_score_id: ranking.best_ex_score_id,
    best_clear_score_id: clear.best_clear_score_id,
    best_max_combo_score_id: combo.best_max_combo_score_id,
    best_min_bp_score_id: bp.best_min_bp_score_id,
    best_min_cb_score_id: cb.best_min_cb_score_id,
  }
}

export function bestClearWins(next: BestScoreRow, current: BestScoreRow): boolean {
  if (next.clear_rank !== current.clear_rank) {
    return next.clear_rank > current.clear_rank
  }
  return (
    String(next.played_at ?? next.server_received_at).localeCompare(
      String(current.played_at ?? current.server_received_at),
    ) < 0
  )
}

export function rankRows(
  rows: BestScoreRow[],
  selfId: string | null,
  rivalIds: Set<string>,
  names: Map<string, string>,
): IrRankingEntry[] {
  const sorted = [...rows].sort(
    (a, b) =>
      b.ex_score - a.ex_score ||
      b.clear_rank - a.clear_rank ||
      a.min_bp - b.min_bp ||
      a.min_cb - b.min_cb ||
      b.max_combo - a.max_combo ||
      String(a.played_at ?? a.server_received_at).localeCompare(
        String(b.played_at ?? b.server_received_at),
      ),
  )
  let previousEx: number | null = null
  let currentRank = 0
  return sorted.map((row, index) => {
    if (previousEx !== row.ex_score) {
      currentRank = index + 1
      previousEx = row.ex_score
    }
    return {
      rank: currentRank,
      scope_rank: index + 1,
      player: {
        id: row.player_id,
        display_name: names.get(row.player_id) || 'Player',
      },
      score: {
        score_id: row.score_id,
        clear: row.clear_type,
        ex_score: row.ex_score,
        max_combo: row.max_combo,
        min_bp: row.min_bp,
        min_cb: row.min_cb,
        gauge: row.gauge,
        ln_policy: row.ln_policy,
        double_option: row.double_option,
        rule_mode: row.rule_mode,
        device_type: row.device_type,
        arrange_1p: row.arrange_1p,
        arrange_2p: row.arrange_2p,
        played_at: row.played_at,
        verification: row.verification,
        judges: row.judges,
        source_score_ids: {
          ex_score: row.best_ex_score_id,
          clear: row.best_clear_score_id,
          max_combo: row.best_max_combo_score_id,
          min_bp: row.best_min_bp_score_id,
          min_cb: row.best_min_cb_score_id,
        },
      },
      relation: {
        is_self: row.player_id === selfId,
        is_rival: rivalIds.has(row.player_id),
      },
    }
  })
}

/** around_self で自分の前後に表示する人数 (自分を含めて最大 2N+1 件)。 */
export const AROUND_SELF_WINDOW = 5

export function applyScope(
  entries: IrRankingEntry[],
  scope: IrRankingScope,
  selfId: string | null,
  rivalIds: Set<string>,
) {
  if (scope === 'global') {
    return entries
  }
  if (scope === 'around_self') {
    // 自分の前後 AROUND_SELF_WINDOW 件ずつを切り出す。未ログイン /
    // 自己スコアなしのときは global と同じ全件を返す。
    const selfIndex = selfId ? entries.findIndex((entry) => entry.player.id === selfId) : -1
    if (selfIndex < 0) {
      return entries
    }
    const start = Math.max(0, selfIndex - AROUND_SELF_WINDOW)
    return entries.slice(start, selfIndex + AROUND_SELF_WINDOW + 1)
  }
  if (scope === 'self') {
    return entries.filter((entry) => entry.player.id === selfId)
  }
  if (scope === 'rivals') {
    return entries.filter((entry) => rivalIds.has(entry.player.id))
  }
  return entries.filter((entry) => entry.player.id === selfId || rivalIds.has(entry.player.id))
}

export async function getRivalIds(playerId: string): Promise<Set<string>> {
  const rows = await db
    .select({ target_player_id: schema.rivalRelationships.targetPlayerId })
    .from(schema.rivalRelationships)
    .where(
      and(
        eq(schema.rivalRelationships.ownerPlayerId, playerId),
        eq(schema.rivalRelationships.relationType, 'rival'),
      ),
    )
  return new Set(rows.map((row) => row.target_player_id))
}

export async function getPlayerNames(playerIds: string[]): Promise<Map<string, string>> {
  if (playerIds.length === 0) {
    return new Map()
  }
  const rows = await db
    .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
    .from(schema.profiles)
    .where(inArray(schema.profiles.id, playerIds))
  return new Map(rows.map((row) => [row.id, row.display_name || 'Player']))
}

/**
 * tamper evidence の署名を検証する。
 *
 * - evidence なし / 署名なし → unverified
 * - 署名ありで device key 不明・hash 不一致・署名不正 → reject
 * - 検証成功 → submission source に応じた verified_play / signed_backfill
 *
 * canonical form は「evidence を除いた payload をキー昇順 compact JSON 化」
 * したもので、BMZ クライアント (serde_json の BTreeMap 出力) と一致させる。
 */
