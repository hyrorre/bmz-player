import { createHash, createPublicKey, randomUUID, verify as cryptoVerify } from 'node:crypto'
import { and, desc, eq, inArray, isNull } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../utils/db_errors'
import type {
  IrChartLnProfile,
  IrDeviceType,
  IrDoubleOption,
  IrRanking,
  IrRankingEntry,
  IrRankingScope,
  IrRuleMode,
  IrScoreSubmission,
  IrSubmitResponse,
  LnScorePolicy,
} from '../../shared/types/ir'

const LN_POLICIES = new Set(['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn'])
const EFFECTIVE_LN_MODES = new Set(['ln', 'cn', 'hcn'])
const DEVICE_TYPES = new Set(['keyboard', 'controller'])
const RULE_MODES = new Set(['Beatoraja', 'Lr2Oraja', 'Dx'])
const RANKING_SCOPES = new Set(['global', 'self_and_rivals', 'rivals', 'self', 'around_self'])
export const CLEAR_RANK: Record<string, number> = {
  no_play: 0,
  NoPlay: 0,
  failed: 1,
  Failed: 1,
  assisted_easy_clear: 2,
  AssistEasy: 2,
  LightAssistEasy: 2,
  easy_clear: 3,
  Easy: 3,
  clear: 4,
  Normal: 4,
  hard_clear: 5,
  Hard: 5,
  ex_hard_clear: 6,
  ExHard: 6,
  full_combo: 7,
  FullCombo: 7,
  perfect: 8,
  Perfect: 8,
  Max: 9,
}

export interface IrRequestUser {
  id: string
}

export interface RankingQuery {
  scope: IrRankingScope
  limit: number
  offset: number
  lnPolicy?: LnScorePolicy
  doubleOption: IrDoubleOption
  ruleMode: IrRuleMode
  scoring: 'bms_ex_score_v1'
}

interface BestScoreCandidate {
  ex_score: number
  clear_rank: number
  max_combo: number
  min_bp: number
  min_cb: number
  server_received_at: Date
}

interface BestScoreRow extends BestScoreCandidate {
  player_id: string
  chart_sha256: string
  score_id: string
  best_ex_score_id: string
  best_clear_score_id: string
  best_max_combo_score_id: string
  best_min_bp_score_id: string
  best_min_cb_score_id: string
  clear_type: string
  gauge: string
  ln_policy: LnScorePolicy
  effective_ln_mode: 'ln' | 'cn' | 'hcn'
  double_option: IrDoubleOption
  rule_mode: IrRuleMode
  scoring: 'bms_ex_score_v1'
  device_type: IrDeviceType
  played_at: string | null
  verification: 'unverified' | 'signed' | 'invalid' | 'trusted'
}

interface ScoreHistoryRankingRow extends Omit<BestScoreRow, 'score_id'> {
  id: string
}

export function parseRankingQuery(query: Record<string, unknown>): RankingQuery {
  const scope = asScope(String(query.scope ?? 'global'))
  const limit = clampInteger(query.limit, 100, 1, 200)
  const offset = clampInteger(query.offset, 0, 0, 100_000)
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy ? asLnPolicy(query.ln_policy) : undefined
  const doubleOption = normalizeDoubleOption(query.double_option)
  const ruleMode = asRuleMode(query.rule_mode)
  const scoring = String(query.scoring ?? 'bms_ex_score_v1')
  if (scoring !== 'bms_ex_score_v1') {
    throw new Error('unsupported scoring')
  }
  return { scope, limit, offset, lnPolicy, doubleOption, ruleMode, scoring }
}

export function parseRankingScope(value: string): IrRankingScope {
  return asScope(value)
}

export function validateScoreSubmission(value: unknown): IrScoreSubmission {
  if (!isRecord(value)) {
    throw new Error('payload must be an object')
  }
  const payload = value as unknown as IrScoreSubmission
  if (!isRecord(payload.client) || !isRecord(payload.chart) || !isRecord(payload.rule)) {
    throw new Error('client, chart, and rule are required')
  }
  if (!isRecord(payload.result)) {
    throw new Error('result is required')
  }
  requireHex(payload.chart.sha256, 64, 'chart.sha256')
  if (payload.chart.md5 != null) {
    requireHex(payload.chart.md5, 32, 'chart.md5')
  }
  if (payload.chart.difficulty != null && typeof payload.chart.difficulty !== 'string') {
    throw new Error('chart.difficulty must be a string')
  }
  asLnPolicy(payload.rule.ln_policy)
  asRuleMode(payload.rule.rule_mode)
  if (!EFFECTIVE_LN_MODES.has(payload.rule.effective_ln_mode)) {
    throw new Error('rule.effective_ln_mode is invalid')
  }
  if (payload.rule.scoring !== 'bms_ex_score_v1') {
    throw new Error('rule.scoring is unsupported')
  }
  for (const field of ['ex_score', 'max_combo', 'notes', 'min_bp', 'min_cb'] as const) {
    requireNonNegativeInteger(payload.result[field], `result.${field}`)
  }
  if (payload.result.pass_notes != null) {
    requireNonNegativeInteger(payload.result.pass_notes, 'result.pass_notes')
  }
  if (
    !payload.result.judges ||
    !isRecord(payload.result.judges.fast) ||
    !isRecord(payload.result.judges.slow)
  ) {
    throw new Error('result.judges.fast and result.judges.slow are required')
  }
  for (const side of ['fast', 'slow'] as const) {
    for (const key of ['pgreat', 'great', 'good', 'bad', 'poor', 'empty_poor'] as const) {
      requireNonNegativeInteger(payload.result.judges[side][key], `result.judges.${side}.${key}`)
    }
  }
  if (!payload.idempotency_key || typeof payload.idempotency_key !== 'string') {
    throw new Error('idempotency_key is required')
  }
  if (!isRecord(payload.play_options)) {
    throw new Error('play_options is required')
  }
  if (!DEVICE_TYPES.has(String(payload.play_options.device_type))) {
    throw new Error('play_options.device_type is invalid')
  }
  normalizeDoubleOption(payload.play_options.double_option)
  return payload
}

export async function submitScore(
  user: IrRequestUser,
  payload: IrScoreSubmission,
  rankingScopes: IrRankingScope[],
  rankingLimit: number,
): Promise<IrSubmitResponse> {
  const doubleOption = normalizeDoubleOption(payload.play_options.double_option)
  await upsertChart(payload, doubleOption === 'off')

  const bp =
    judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor') + judgeTotal(payload, 'empty_poor')
  const cb = judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor')
  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const verification = await resolveVerification(user.id, payload)
  const deviceType = payload.play_options.device_type

  const scoreId = randomUUID()
  const scoreInsert = {
    id: scoreId,
    playerId: user.id,
    chartSha256: payload.chart.sha256,
    clientName: payload.client.name,
    clientVersion: payload.client.version,
    platform: payload.client.platform,
    playMode: payload.rule.play_mode,
    keyMode: payload.rule.key_mode,
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
    effectiveLnMode: payload.rule.effective_ln_mode,
    ruleMode: payload.rule.rule_mode,
    judgeAlgorithm: payload.rule.judge_algorithm,
    scoring: payload.rule.scoring,
    clearType: payload.result.clear,
    clearRank,
    playedAt: playedAtDate(payload.result.played_at),
    durationMs: payload.result.duration_ms ?? null,
    judges: payload.result.judges,
    exScore: payload.result.ex_score,
    avgJudgeMs: payload.result.avg_judge_ms ?? null,
    maxCombo: payload.result.max_combo,
    notes: payload.result.notes,
    passNotes: payload.result.pass_notes ?? payload.result.notes,
    bp,
    cb,
    minBp: payload.result.min_bp,
    minCb: payload.result.min_cb,
    deviceType,
    doubleOption,
    playOptions: { ...payload.play_options, double_option: doubleOption } as Record<
      string,
      unknown
    >,
    replayHash: payload.replay?.hash ?? null,
    replayFormat: payload.replay?.format ?? null,
    replayUploadIntent: payload.replay?.upload_intent ?? null,
    evidence: payload.evidence ?? {},
    verification,
    idempotencyKey: payload.idempotency_key,
  }

  let score = await insertScore(scoreInsert)
  if (!score) {
    score =
      (await db.query.scores.findFirst({
        columns: { id: true, serverReceivedAt: true },
        where: and(
          eq(schema.scores.playerId, user.id),
          eq(schema.scores.idempotencyKey, payload.idempotency_key),
        ),
      })) ?? null
  }
  if (!score) {
    throw new Error('failed to insert score')
  }

  const candidate: BestScoreCandidate = {
    ex_score: payload.result.ex_score,
    clear_rank: clearRank,
    max_combo: payload.result.max_combo,
    min_bp: payload.result.min_bp,
    min_cb: payload.result.min_cb,
    server_received_at: score.serverReceivedAt,
  }
  const previousBest = await fetchPreviousBest(user.id, payload)
  const { bestUpdated, updatedFields } = await upsertBestScore(
    user.id,
    payload,
    score.id,
    verification,
    candidate,
  )

  const rankings: IrSubmitResponse['rankings'] = {}
  for (const scope of rankingScopes) {
    try {
      rankings[scope] = {
        succeeded: true,
        data: await getRanking(user, payload.chart.sha256, {
          scope,
          limit: rankingLimit,
          offset: 0,
          lnPolicy: payload.rule.ln_policy,
          doubleOption,
          ruleMode: payload.rule.rule_mode,
          scoring: payload.rule.scoring,
        }),
      }
    } catch (error) {
      rankings[scope] = {
        succeeded: false,
        error: error instanceof Error ? error.message : 'ranking failed',
      }
    }
  }

  return {
    accepted: true,
    score_id: score.id,
    best_updated: bestUpdated,
    updated_fields: updatedFields,
    server_received_at: score.serverReceivedAt.toISOString(),
    previous_best: previousBest,
    rankings: Object.keys(rankings).length > 0 ? rankings : undefined,
  }
}

export async function getRanking(
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
): Promise<IrRanking> {
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
  }
}

async function fetchRankingBestRows(sha256: string, query: RankingQuery): Promise<BestScoreRow[]> {
  const conditions = [
    eq(schema.bestScores.chartSha256, sha256),
    eq(schema.bestScores.scoring, query.scoring),
    eq(schema.bestScores.doubleOption, query.doubleOption),
    eq(schema.bestScores.ruleMode, query.ruleMode),
  ]
  if (query.lnPolicy) {
    conditions.push(eq(schema.bestScores.lnPolicy, query.lnPolicy))
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
    return cachedRows
  }
  return fetchRankingBestRowsFromHistory(sha256, query)
}

async function fetchRankingBestRowsFromHistory(
  sha256: string,
  query: RankingQuery,
): Promise<BestScoreRow[]> {
  const conditions = [
    eq(schema.scores.chartSha256, sha256),
    eq(schema.scores.scoring, query.scoring),
    eq(schema.scores.doubleOption, query.doubleOption),
    eq(schema.scores.ruleMode, query.ruleMode),
    eq(schema.scores.accepted, true),
  ]
  if (query.lnPolicy) {
    conditions.push(eq(schema.scores.lnPolicy, query.lnPolicy))
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
    })
    .from(schema.scores)
    .where(and(...conditions))
    .orderBy(desc(schema.scores.exScore))

  return bestRowsFromHistory(
    rows.map((row) => ({ ...rowToBestScoreRow({ ...row, score_id: row.id }), id: row.id })),
  )
}

function rowToBestScoreRow(row: {
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
}): BestScoreRow {
  return {
    ...row,
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
  }
}

function bestRowsFromHistory(rows: ScoreHistoryRankingRow[]): BestScoreRow[] {
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

function historyRowToBestRow(row: ScoreHistoryRankingRow): BestScoreRow {
  const { id, ...score } = row
  return { ...score, score_id: id }
}

function bestRowKey(row: BestScoreRow): string {
  return [
    row.player_id,
    row.chart_sha256,
    row.ln_policy,
    row.scoring,
    row.double_option,
    row.rule_mode,
  ].join('\0')
}

function bestRowWins(next: BestScoreRow, current: BestScoreRow): boolean {
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

function dedupeBestRowsByPlayer(rows: BestScoreRow[]): BestScoreRow[] {
  const bestByPlayer = new Map<string, BestScoreRow>()
  for (const row of rows) {
    const current = bestByPlayer.get(row.player_id)
    bestByPlayer.set(row.player_id, current ? mergeBestRows(current, row) : row)
  }
  return [...bestByPlayer.values()]
}

function mergeBestRows(current: BestScoreRow, next: BestScoreRow): BestScoreRow {
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

function bestClearWins(next: BestScoreRow, current: BestScoreRow): boolean {
  if (next.clear_rank !== current.clear_rank) {
    return next.clear_rank > current.clear_rank
  }
  return (
    String(next.played_at ?? next.server_received_at).localeCompare(
      String(current.played_at ?? current.server_received_at),
    ) < 0
  )
}

async function upsertChart(payload: IrScoreSubmission, allowUpdate: boolean) {
  const profile: Partial<IrChartLnProfile> = payload.chart.ln_profile ?? {}
  const notes = payload.chart.notes ?? {}
  const features = payload.chart.features ?? {}
  const values = {
    sha256: payload.chart.sha256,
    md5: payload.chart.md5 ?? null,
    title: payload.chart.title ?? '',
    subtitle: payload.chart.subtitle ?? null,
    genre: payload.chart.genre ?? null,
    artist: payload.chart.artist ?? null,
    subartists: payload.chart.subartists ?? [],
    mode: payload.chart.mode ?? payload.rule.key_mode ?? 'unknown',
    level: payload.chart.level ?? null,
    difficulty: payload.chart.difficulty ?? null,
    total: payload.chart.total ?? null,
    judgeRank: payload.chart.judge ?? null,
    minBpm: payload.chart.bpm?.min ?? null,
    maxBpm: payload.chart.bpm?.max ?? null,
    notes: notes.total ?? payload.result.notes,
    lnNotes: notes.ln ?? 0,
    cnNotes: notes.cn ?? 0,
    hcnNotes: notes.hcn ?? 0,
    mineNotes: notes.mine ?? 0,
    hasRandom: features.random ?? false,
    hasStop: features.stop ?? false,
    hasUndefinedLn: profile.has_undefined_ln ?? false,
    hasDefinedLn: profile.has_defined_ln ?? false,
    hasDefinedCn: profile.has_defined_cn ?? false,
    hasDefinedHcn: profile.has_defined_hcn ?? false,
    hasLn: features.ln ?? profile.has_defined_ln ?? profile.has_undefined_ln ?? false,
    hasCn: features.cn ?? profile.has_defined_cn ?? false,
    hasHcn: features.hcn ?? profile.has_defined_hcn ?? false,
    hasMine: features.mine ?? false,
    sourceUrl: payload.chart.urls?.source ?? null,
    appendUrl: payload.chart.urls?.append ?? null,
    headers: {},
    updatedAt: new Date(),
  }

  if (!allowUpdate) {
    await db.insert(schema.charts).values(values).onConflictDoNothing()
    return
  }
  await db
    .insert(schema.charts)
    .values(values)
    .onConflictDoUpdate({ target: schema.charts.sha256, set: values })
}

async function fetchPreviousBest(
  playerId: string,
  payload: IrScoreSubmission,
): Promise<IrSubmitResponse['previous_best']> {
  const current = await db.query.bestScores.findFirst({
    columns: { exScore: true, clearType: true, maxCombo: true, minBp: true, minCb: true },
    where: and(
      eq(schema.bestScores.playerId, playerId),
      eq(schema.bestScores.chartSha256, payload.chart.sha256),
      eq(schema.bestScores.lnPolicy, payload.rule.ln_policy),
      eq(schema.bestScores.scoring, payload.rule.scoring),
      eq(schema.bestScores.doubleOption, normalizeDoubleOption(payload.play_options.double_option)),
      eq(schema.bestScores.ruleMode, payload.rule.rule_mode),
    ),
  })
  if (!current) {
    return null
  }
  return {
    clear_type: current.clearType,
    ex_score: current.exScore,
    max_combo: current.maxCombo,
    min_bp: current.minBp,
    min_cb: current.minCb,
  }
}

async function upsertBestScore(
  playerId: string,
  payload: IrScoreSubmission,
  scoreId: string,
  verification: string,
  candidate: BestScoreCandidate,
) {
  const current = await db.query.bestScores.findFirst({
    columns: {
      scoreId: true,
      exScore: true,
      clearType: true,
      clearRank: true,
      maxCombo: true,
      minBp: true,
      minCb: true,
      deviceType: true,
      gauge: true,
      effectiveLnMode: true,
      playedAt: true,
      serverReceivedAt: true,
      verification: true,
      bestExScoreId: true,
      bestClearScoreId: true,
      bestMaxComboScoreId: true,
      bestMinBpScoreId: true,
      bestMinCbScoreId: true,
    },
    where: and(
      eq(schema.bestScores.playerId, playerId),
      eq(schema.bestScores.chartSha256, payload.chart.sha256),
      eq(schema.bestScores.lnPolicy, payload.rule.ln_policy),
      eq(schema.bestScores.scoring, payload.rule.scoring),
      eq(schema.bestScores.doubleOption, normalizeDoubleOption(payload.play_options.double_option)),
      eq(schema.bestScores.ruleMode, payload.rule.rule_mode),
    ),
  })
  const currentCandidate = current
    ? {
        ex_score: current.exScore,
        clear_rank: current.clearRank,
        max_combo: current.maxCombo,
        min_bp: current.minBp,
        min_cb: current.minCb,
        server_received_at: current.serverReceivedAt,
      }
    : null

  const updatedFields = {
    ex_score: !currentCandidate || candidate.ex_score > currentCandidate.ex_score,
    clear: !currentCandidate || candidate.clear_rank > currentCandidate.clear_rank,
    max_combo: !currentCandidate || candidate.max_combo > currentCandidate.max_combo,
    min_bp: !currentCandidate || candidate.min_bp < currentCandidate.min_bp,
    min_cb: !currentCandidate || candidate.min_cb < currentCandidate.min_cb,
  }
  const rankingUpdated = !currentCandidate || bestCandidateWins(candidate, currentCandidate)
  const shouldUpdate =
    rankingUpdated ||
    updatedFields.clear ||
    updatedFields.max_combo ||
    updatedFields.min_bp ||
    updatedFields.min_cb
  if (!shouldUpdate) {
    return { bestUpdated: false, updatedFields }
  }

  const verificationStatus = verification as 'unverified' | 'signed' | 'invalid' | 'trusted'
  const playedAt = playedAtDate(payload.result.played_at)
  const values = {
    id: randomUUID(),
    playerId,
    chartSha256: payload.chart.sha256,
    scoreId: rankingUpdated ? scoreId : (current?.scoreId ?? scoreId),
    bestExScoreId: rankingUpdated
      ? scoreId
      : (current?.bestExScoreId ?? current?.scoreId ?? scoreId),
    bestClearScoreId: updatedFields.clear
      ? scoreId
      : (current?.bestClearScoreId ?? current?.scoreId ?? scoreId),
    bestMaxComboScoreId: updatedFields.max_combo
      ? scoreId
      : (current?.bestMaxComboScoreId ?? current?.scoreId ?? scoreId),
    bestMinBpScoreId: updatedFields.min_bp
      ? scoreId
      : (current?.bestMinBpScoreId ?? current?.scoreId ?? scoreId),
    bestMinCbScoreId: updatedFields.min_cb
      ? scoreId
      : (current?.bestMinCbScoreId ?? current?.scoreId ?? scoreId),
    exScore: rankingUpdated ? candidate.ex_score : (current?.exScore ?? candidate.ex_score),
    clearType: updatedFields.clear
      ? payload.result.clear
      : (current?.clearType ?? payload.result.clear),
    clearRank: updatedFields.clear
      ? candidate.clear_rank
      : (current?.clearRank ?? candidate.clear_rank),
    maxCombo: updatedFields.max_combo
      ? candidate.max_combo
      : (current?.maxCombo ?? candidate.max_combo),
    minBp: updatedFields.min_bp ? candidate.min_bp : (current?.minBp ?? candidate.min_bp),
    minCb: updatedFields.min_cb ? candidate.min_cb : (current?.minCb ?? candidate.min_cb),
    deviceType: rankingUpdated
      ? payload.play_options.device_type
      : (current?.deviceType ?? payload.play_options.device_type),
    doubleOption: normalizeDoubleOption(payload.play_options.double_option),
    gauge: rankingUpdated ? payload.rule.gauge : (current?.gauge ?? payload.rule.gauge),
    lnPolicy: payload.rule.ln_policy,
    effectiveLnMode: rankingUpdated
      ? payload.rule.effective_ln_mode
      : (current?.effectiveLnMode ?? payload.rule.effective_ln_mode),
    ruleMode: payload.rule.rule_mode,
    scoring: payload.rule.scoring,
    playedAt: rankingUpdated ? playedAt : (current?.playedAt ?? playedAt),
    serverReceivedAt: rankingUpdated
      ? candidate.server_received_at
      : (current?.serverReceivedAt ?? candidate.server_received_at),
    verification: rankingUpdated
      ? verificationStatus
      : (current?.verification ?? verificationStatus),
  }
  await db
    .insert(schema.bestScores)
    .values(values)
    .onConflictDoUpdate({
      target: [
        schema.bestScores.playerId,
        schema.bestScores.chartSha256,
        schema.bestScores.lnPolicy,
        schema.bestScores.scoring,
        schema.bestScores.doubleOption,
        schema.bestScores.ruleMode,
      ],
      set: {
        scoreId: values.scoreId,
        bestExScoreId: values.bestExScoreId,
        bestClearScoreId: values.bestClearScoreId,
        bestMaxComboScoreId: values.bestMaxComboScoreId,
        bestMinBpScoreId: values.bestMinBpScoreId,
        bestMinCbScoreId: values.bestMinCbScoreId,
        exScore: values.exScore,
        clearType: values.clearType,
        clearRank: values.clearRank,
        maxCombo: values.maxCombo,
        minBp: values.minBp,
        minCb: values.minCb,
        deviceType: values.deviceType,
        effectiveLnMode: values.effectiveLnMode,
        playedAt: values.playedAt,
        serverReceivedAt: values.serverReceivedAt,
        verification: values.verification,
        updatedAt: new Date(),
      },
    })
  return { bestUpdated: true, updatedFields }
}

async function insertScore(values: typeof schema.scores.$inferInsert) {
  try {
    const [inserted] = await db
      .insert(schema.scores)
      .values(values)
      .returning({ id: schema.scores.id, serverReceivedAt: schema.scores.serverReceivedAt })
    return inserted ?? null
  } catch (error) {
    if (isUniqueConstraintError(error)) {
      return null
    }
    throw error
  }
}

function bestCandidateWins(next: BestScoreCandidate, current: BestScoreCandidate): boolean {
  return (
    next.ex_score > current.ex_score ||
    (next.ex_score === current.ex_score && next.clear_rank > current.clear_rank) ||
    (next.ex_score === current.ex_score &&
      next.clear_rank === current.clear_rank &&
      next.min_bp < current.min_bp) ||
    (next.ex_score === current.ex_score &&
      next.clear_rank === current.clear_rank &&
      next.min_bp === current.min_bp &&
      next.min_cb < current.min_cb) ||
    (next.ex_score === current.ex_score &&
      next.clear_rank === current.clear_rank &&
      next.min_bp === current.min_bp &&
      next.min_cb === current.min_cb &&
      next.max_combo > current.max_combo)
  )
}

function rankRows(
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
        played_at: row.played_at,
        verification: row.verification,
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
const AROUND_SELF_WINDOW = 5

function applyScope(
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

async function getRivalIds(playerId: string): Promise<Set<string>> {
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

async function getPlayerNames(playerIds: string[]): Promise<Map<string, string>> {
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
 * - 署名ありで device key 不明・hash 不一致・署名不正 → invalid
 * - 検証成功 → signed
 *
 * canonical form は「evidence を除いた payload をキー昇順 compact JSON 化」
 * したもので、BMZ クライアント (serde_json の BTreeMap 出力) と一致させる。
 */
export async function resolveVerification(
  playerIdOrDb: string | unknown,
  payloadOrPlayerId: { evidence?: Record<string, unknown> } | string,
  maybePayload?: { evidence?: Record<string, unknown> },
): Promise<'unverified' | 'signed' | 'invalid'> {
  const playerId = typeof playerIdOrDb === 'string' ? playerIdOrDb : String(payloadOrPlayerId)
  const payload =
    typeof playerIdOrDb === 'string'
      ? (payloadOrPlayerId as { evidence?: Record<string, unknown> })
      : (maybePayload ?? {})
  const evidence = payload.evidence
  if (!evidence || typeof evidence !== 'object') {
    return 'unverified'
  }
  const signature = evidence.client_signature
  const keyId = evidence.public_key_id
  const claimedHash = evidence.canonical_hash
  if (!signature) {
    return 'unverified'
  }
  if (
    typeof signature !== 'string' ||
    typeof keyId !== 'string' ||
    typeof claimedHash !== 'string'
  ) {
    return 'invalid'
  }

  const key = await db.query.deviceKeys.findFirst({
    columns: { publicKey: true },
    where: and(
      eq(schema.deviceKeys.id, keyId),
      eq(schema.deviceKeys.playerId, playerId),
      isNull(schema.deviceKeys.revokedAt),
    ),
  })
  if (!key) {
    return 'invalid'
  }

  const hash = createHash('sha256').update(canonicalSubmissionJson(payload)).digest()
  if (hash.toString('hex') !== claimedHash.toLowerCase()) {
    return 'invalid'
  }

  try {
    // Ed25519 raw public key (32 bytes) を SPKI DER に包んで検証する。
    const der = Buffer.concat([
      Buffer.from('302a300506032b6570032100', 'hex'),
      Buffer.from(key.publicKey, 'hex'),
    ])
    const publicKey = createPublicKey({ key: der, format: 'der', type: 'spki' })
    const signatureBytes = Buffer.from(signature, 'base64url')
    return cryptoVerify(null, hash, publicKey, signatureBytes) ? 'signed' : 'invalid'
  } catch {
    return 'invalid'
  }
}

function canonicalSubmissionJson(payload: { evidence?: Record<string, unknown> }): string {
  const clone: Record<string, unknown> = { ...payload }
  delete clone.evidence
  return stableStringify(clone)
}

/** キー昇順・空白なしの決定的 JSON 文字列化。 */
export function stableStringify(value: unknown): string {
  if (value === undefined) {
    throw new Error('canonical JSON does not support undefined')
  }
  if (typeof value === 'number' && !Number.isFinite(value)) {
    throw new Error('canonical JSON number must be finite')
  }
  if (value === null || typeof value !== 'object') {
    const serialized = JSON.stringify(value)
    if (serialized === undefined) {
      throw new Error('canonical JSON value is not serializable')
    }
    return serialized
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(',')}]`
  }
  const record = value as Record<string, unknown>
  const parts = Object.keys(record)
    .filter((key) => record[key] !== undefined)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`)
  return `{${parts.join(',')}}`
}

/** played_at は ISO 文字列または unix 秒 (BMZ client) を受け付ける。 */
function playedAtDate(value: unknown): Date | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Date(value * 1000)
  }
  if (typeof value === 'string' && value.length > 0) {
    return new Date(value)
  }
  return null
}

function judgeTotal(
  payload: IrScoreSubmission,
  key: keyof IrScoreSubmission['result']['judges']['fast'],
): number {
  return payload.result.judges.fast[key] + payload.result.judges.slow[key]
}

function asLnPolicy(value: string): LnScorePolicy {
  if (!LN_POLICIES.has(value)) {
    throw new Error('ln_policy is invalid')
  }
  return value as LnScorePolicy
}

function asRuleMode(value: unknown): IrRuleMode {
  if (typeof value !== 'string' || !RULE_MODES.has(value)) {
    throw new Error('rule_mode is invalid')
  }
  return value as IrRuleMode
}

function asScope(value: string): IrRankingScope {
  if (!RANKING_SCOPES.has(value)) {
    throw new Error('scope is invalid')
  }
  return value as IrRankingScope
}

function normalizeDoubleOption(value: unknown): IrDoubleOption {
  const normalized = String(value ?? 'off')
    .trim()
    .toLowerCase()
    .replaceAll('-', '_')

  switch (normalized) {
    case '':
    case 'off':
    case 'flip':
      return 'off'
    case 'battle':
      return 'battle'
    case 'battle_auto_scratch':
    case 'battle_assist':
      return 'battle_auto_scratch'
    default:
      throw new Error('double_option is invalid')
  }
}

function clampInteger(value: unknown, fallback: number, min: number, max: number): number {
  const parsed = Number(value ?? fallback)
  if (!Number.isFinite(parsed)) {
    return fallback
  }
  return Math.max(min, Math.min(max, Math.trunc(parsed)))
}

function nonEmptyString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.length > 0 ? value : fallback
}

export function requireHex(value: unknown, length: number, label: string) {
  if (typeof value !== 'string' || !new RegExp(`^[0-9a-f]{${length}}$`).test(value)) {
    throw new Error(`${label} must be lowercase hex length ${length}`)
  }
}

export function requireNonNegativeInteger(value: unknown, label: string) {
  if (!Number.isInteger(value) || Number(value) < 0) {
    throw new Error(`${label} must be a non-negative integer`)
  }
}

export function requireFiniteNumber(value: unknown, label: string) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`${label} must be a finite number`)
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function normalizeGaugeName(value: string): string {
  const normalized = value.trim().toLowerCase().replaceAll('-', '_')
  switch (normalized) {
    case 'assist_easy':
    case 'a_easy':
      return 'AssistEasy'
    case 'easy':
      return 'Easy'
    case 'normal':
      return 'Normal'
    case 'hard':
      return 'Hard'
    case 'ex_hard':
    case 'exhard':
      return 'ExHard'
    case 'hazard':
      return 'Hazard'
    case 'class':
      return 'Class'
    case 'ex_class':
    case 'exclass':
      return 'ExClass'
    case 'ex_hard_class':
    case 'exhardclass':
      return 'ExHardClass'
    default:
      return value
  }
}

export const __test = {
  dedupeBestRowsByPlayer,
  bestRowsFromHistory,
}
