import { createHash, createPublicKey, verify as cryptoVerify } from 'node:crypto'
import type { SupabaseClient } from '@supabase/supabase-js'
import type {
  IrChartLnProfile,
  IrDeviceType,
  IrDoubleOption,
  IrRanking,
  IrRankingEntry,
  IrRankingScope,
  IrScoreSubmission,
  IrSubmitResponse,
  LnScorePolicy,
} from '../../shared/types/ir'
import type { Database, Json } from '../../shared/types/database.types'

const LN_POLICIES = new Set(['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn'])
const EFFECTIVE_LN_MODES = new Set(['ln', 'cn', 'hcn'])
const DEVICE_TYPES = new Set(['keyboard', 'controller'])
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

type Db = SupabaseClient<Database>

export interface IrRequestUser {
  id: string
}

export interface RankingQuery {
  scope: IrRankingScope
  limit: number
  offset: number
  gauge?: string
  lnPolicy?: LnScorePolicy
  doubleOption: IrDoubleOption
  scoring: 'bms_ex_score_v1'
}

interface BestScoreCandidate {
  ex_score: number
  clear_rank: number
  max_combo: number
  min_bp: number
  min_cb: number
  server_received_at: string
}

interface BestScoreRow extends BestScoreCandidate {
  player_id: string
  chart_sha256: string
  score_id: string
  clear_type: string
  gauge: string
  ln_policy: LnScorePolicy
  effective_ln_mode: 'ln' | 'cn' | 'hcn'
  double_option: IrDoubleOption
  scoring: 'bms_ex_score_v1'
  device_type: IrDeviceType
  played_at: string | null
  verification: 'unverified' | 'signed' | 'invalid' | 'trusted'
}

export function parseRankingQuery(query: Record<string, unknown>): RankingQuery {
  const scope = asScope(String(query.scope ?? 'global'))
  const limit = clampInteger(query.limit, 100, 1, 200)
  const offset = clampInteger(query.offset, 0, 0, 100_000)
  const gauge =
    typeof query.gauge === 'string' && query.gauge ? normalizeGaugeName(query.gauge) : undefined
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy ? asLnPolicy(query.ln_policy) : undefined
  const doubleOption = normalizeDoubleOption(query.double_option)
  const scoring = String(query.scoring ?? 'bms_ex_score_v1')
  if (scoring !== 'bms_ex_score_v1') {
    throw new Error('unsupported scoring')
  }
  return { scope, limit, offset, gauge, lnPolicy, doubleOption, scoring }
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
  asLnPolicy(payload.rule.ln_policy)
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
  db: Db,
  user: IrRequestUser,
  payload: IrScoreSubmission,
  rankingScopes: IrRankingScope[],
  rankingLimit: number,
): Promise<IrSubmitResponse> {
  const doubleOption = normalizeDoubleOption(payload.play_options.double_option)
  await upsertChart(db, payload, doubleOption === 'off')

  const bp =
    judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor') + judgeTotal(payload, 'empty_poor')
  const cb = judgeTotal(payload, 'bad') + judgeTotal(payload, 'poor')
  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const verification = await resolveVerification(db, user.id, payload)
  const deviceType = payload.play_options.device_type

  const scoreInsert = {
    player_id: user.id,
    chart_sha256: payload.chart.sha256,
    client_name: payload.client.name,
    client_version: payload.client.version,
    platform: payload.client.platform,
    play_mode: payload.rule.play_mode,
    key_mode: payload.rule.key_mode,
    gauge: payload.rule.gauge,
    ln_policy: payload.rule.ln_policy,
    effective_ln_mode: payload.rule.effective_ln_mode,
    judge_algorithm: payload.rule.judge_algorithm,
    scoring: payload.rule.scoring,
    clear_type: payload.result.clear,
    clear_rank: clearRank,
    played_at: playedAtIso(payload.result.played_at),
    duration_ms: payload.result.duration_ms ?? null,
    judges: payload.result.judges as unknown as Json,
    ex_score: payload.result.ex_score,
    avg_judge_ms: payload.result.avg_judge_ms ?? null,
    max_combo: payload.result.max_combo,
    notes: payload.result.notes,
    pass_notes: payload.result.pass_notes ?? payload.result.notes,
    bp,
    cb,
    min_bp: payload.result.min_bp,
    min_cb: payload.result.min_cb,
    device_type: deviceType,
    double_option: doubleOption,
    play_options: { ...payload.play_options, double_option: doubleOption } as Json,
    replay_hash: payload.replay?.hash ?? null,
    replay_format: payload.replay?.format ?? null,
    replay_upload_intent: payload.replay?.upload_intent ?? null,
    evidence: (payload.evidence ?? {}) as Json,
    verification,
    idempotency_key: payload.idempotency_key,
  }

  const { data: insertedScore, error: insertError } = await db
    .from('scores')
    .insert(scoreInsert)
    .select('id, server_received_at')
    .single()

  let score = insertedScore
  if (insertError) {
    const { data: existing, error: existingError } = await db
      .from('scores')
      .select('id, server_received_at')
      .eq('player_id', user.id)
      .eq('idempotency_key', payload.idempotency_key)
      .maybeSingle()
    if (existingError || !existing) {
      throw insertError
    }
    score = existing
  }
  if (!score) {
    throw insertError ?? new Error('failed to insert score')
  }

  const candidate: BestScoreCandidate = {
    ex_score: payload.result.ex_score,
    clear_rank: clearRank,
    max_combo: payload.result.max_combo,
    min_bp: payload.result.min_bp,
    min_cb: payload.result.min_cb,
    server_received_at: score.server_received_at,
  }
  const previousBest = await fetchPreviousBest(db, user.id, payload)
  // 署名検証に失敗した投稿は改ざんの積極的な証拠なので、履歴 (scores) には
  // verification=invalid で残すが best 更新の対象にはしない。
  const { bestUpdated, updatedFields } =
    verification === 'invalid'
      ? {
          bestUpdated: false,
          updatedFields: {
            ex_score: false,
            clear: false,
            max_combo: false,
            min_bp: false,
            min_cb: false,
          },
        }
      : await upsertBestScore(db, user.id, payload, score.id, verification, candidate)

  const rankings: IrSubmitResponse['rankings'] = {}
  for (const scope of rankingScopes) {
    try {
      rankings[scope] = {
        succeeded: true,
        data: await getRanking(db, user, payload.chart.sha256, {
          scope,
          limit: rankingLimit,
          offset: 0,
          gauge: payload.rule.gauge,
          lnPolicy: payload.rule.ln_policy,
          doubleOption,
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
    server_received_at: score.server_received_at,
    previous_best: previousBest,
    rankings: Object.keys(rankings).length > 0 ? rankings : undefined,
  }
}

export async function getRanking(
  db: Db,
  user: IrRequestUser | null,
  sha256: string,
  query: RankingQuery,
): Promise<IrRanking> {
  requireHex(sha256, 64, 'sha256')
  let rankingQuery = db
    .from('best_scores')
    .select(
      'player_id, chart_sha256, score_id, ex_score, clear_type, clear_rank, max_combo, min_bp, min_cb, device_type, gauge, ln_policy, effective_ln_mode, double_option, scoring, played_at, server_received_at, verification',
    )
    .eq('chart_sha256', sha256)
    .eq('scoring', query.scoring)
    .eq('double_option', query.doubleOption)
    .order('ex_score', { ascending: false })
  if (query.gauge) {
    rankingQuery = rankingQuery.eq('gauge', query.gauge)
  }
  if (query.lnPolicy) {
    rankingQuery = rankingQuery.eq('ln_policy', query.lnPolicy)
  }
  const { data: rows, error } = await rankingQuery

  if (error) {
    throw error
  }

  const bestRows = (rows ?? []) as BestScoreRow[]
  const rivalIds = user ? await getRivalIds(db, user.id) : new Set<string>()
  const playerIds = [...new Set(bestRows.map((row) => row.player_id))]
  const names = await getPlayerNames(db, playerIds)
  const ranked = rankRows(bestRows, user?.id ?? null, rivalIds, names)
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
      gauge: query.gauge,
      ln_policy: query.lnPolicy,
      effective_ln_mode: query.lnPolicy
        ? bestRows.find((row) => row.ln_policy === query.lnPolicy)?.effective_ln_mode
        : undefined,
      double_option: query.doubleOption,
    },
    ranking: {
      scope: query.scope,
      sort: 'ex_score_desc',
      // 全プレイヤー中のクリア率 (%)。NoPlay/Failed を除いた割合。
      clear_rate:
        bestRows.length > 0
          ? Math.round(
              (bestRows.filter((row) => row.clear_rank > 1).length / bestRows.length) * 100,
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

async function upsertChart(db: Db, payload: IrScoreSubmission, allowUpdate: boolean) {
  const profile: Partial<IrChartLnProfile> = payload.chart.ln_profile ?? {}
  const notes = payload.chart.notes ?? {}
  const features = payload.chart.features ?? {}
  const { error } = await db.from('charts').upsert(
    {
      sha256: payload.chart.sha256,
      md5: payload.chart.md5 ?? null,
      title: payload.chart.title ?? '',
      subtitle: payload.chart.subtitle ?? null,
      genre: payload.chart.genre ?? null,
      artist: payload.chart.artist ?? null,
      subartists: payload.chart.subartists ?? [],
      mode: payload.chart.mode ?? payload.rule.key_mode ?? 'unknown',
      level: payload.chart.level ?? null,
      total: payload.chart.total ?? null,
      judge_rank: payload.chart.judge ?? null,
      min_bpm: payload.chart.bpm?.min ?? null,
      max_bpm: payload.chart.bpm?.max ?? null,
      notes: notes.total ?? payload.result.notes,
      ln_notes: notes.ln ?? 0,
      cn_notes: notes.cn ?? 0,
      hcn_notes: notes.hcn ?? 0,
      mine_notes: notes.mine ?? 0,
      has_random: features.random ?? false,
      has_stop: features.stop ?? false,
      has_undefined_ln: profile.has_undefined_ln ?? false,
      has_defined_ln: profile.has_defined_ln ?? false,
      has_defined_cn: profile.has_defined_cn ?? false,
      has_defined_hcn: profile.has_defined_hcn ?? false,
      has_ln: features.ln ?? profile.has_defined_ln ?? profile.has_undefined_ln ?? false,
      has_cn: features.cn ?? profile.has_defined_cn ?? false,
      has_hcn: features.hcn ?? profile.has_defined_hcn ?? false,
      has_mine: features.mine ?? false,
      source_url: payload.chart.urls?.source ?? null,
      append_url: payload.chart.urls?.append ?? null,
      headers: payload.chart.headers ?? {},
    },
    {
      onConflict: 'sha256',
      // Battle系は result.notes が2倍になり、key_mode もDP化されるため、
      // 既存の元譜面 chart metadata を上書きしない。
      ignoreDuplicates: !allowUpdate,
    },
  )
  if (error) {
    throw error
  }
}

async function fetchPreviousBest(
  db: Db,
  playerId: string,
  payload: IrScoreSubmission,
): Promise<IrSubmitResponse['previous_best']> {
  const { data: current, error } = await db
    .from('best_scores')
    .select('ex_score, clear_type, max_combo, min_bp, min_cb')
    .eq('player_id', playerId)
    .eq('chart_sha256', payload.chart.sha256)
    .eq('gauge', payload.rule.gauge)
    .eq('ln_policy', payload.rule.ln_policy)
    .eq('scoring', payload.rule.scoring)
    .eq('double_option', normalizeDoubleOption(payload.play_options.double_option))
    .maybeSingle()
  if (error) {
    throw error
  }
  if (!current) {
    return null
  }
  return {
    clear_type: current.clear_type,
    ex_score: current.ex_score,
    max_combo: current.max_combo,
    min_bp: current.min_bp,
    min_cb: current.min_cb,
  }
}

async function upsertBestScore(
  db: Db,
  playerId: string,
  payload: IrScoreSubmission,
  scoreId: string,
  verification: string,
  candidate: BestScoreCandidate,
) {
  const { data: current, error: currentError } = await db
    .from('best_scores')
    .select('ex_score, clear_rank, max_combo, min_bp, min_cb, server_received_at')
    .eq('player_id', playerId)
    .eq('chart_sha256', payload.chart.sha256)
    .eq('gauge', payload.rule.gauge)
    .eq('ln_policy', payload.rule.ln_policy)
    .eq('scoring', payload.rule.scoring)
    .eq('double_option', normalizeDoubleOption(payload.play_options.double_option))
    .maybeSingle()
  if (currentError) {
    throw currentError
  }

  const updatedFields = {
    ex_score: !current || candidate.ex_score > current.ex_score,
    clear: !current || candidate.clear_rank > current.clear_rank,
    max_combo: !current || candidate.max_combo > current.max_combo,
    min_bp: !current || candidate.min_bp < current.min_bp,
    min_cb: !current || candidate.min_cb < current.min_cb,
  }
  const shouldUpdate = !current || bestCandidateWins(candidate, current as BestScoreCandidate)
  if (!shouldUpdate) {
    return { bestUpdated: false, updatedFields }
  }

  const { error } = await db.from('best_scores').upsert(
    {
      player_id: playerId,
      chart_sha256: payload.chart.sha256,
      score_id: scoreId,
      ex_score: candidate.ex_score,
      clear_type: payload.result.clear,
      clear_rank: candidate.clear_rank,
      max_combo: candidate.max_combo,
      min_bp: candidate.min_bp,
      min_cb: candidate.min_cb,
      device_type: payload.play_options.device_type,
      double_option: normalizeDoubleOption(payload.play_options.double_option),
      gauge: payload.rule.gauge,
      ln_policy: payload.rule.ln_policy,
      effective_ln_mode: payload.rule.effective_ln_mode,
      scoring: payload.rule.scoring,
      played_at: playedAtIso(payload.result.played_at),
      server_received_at: candidate.server_received_at,
      verification,
    },
    { onConflict: 'player_id,chart_sha256,gauge,ln_policy,scoring,double_option' },
  )
  if (error) {
    throw error
  }
  return { bestUpdated: true, updatedFields }
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
        device_type: row.device_type,
        played_at: row.played_at,
        verification: row.verification,
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

async function getRivalIds(db: Db, playerId: string): Promise<Set<string>> {
  const { data, error } = await db
    .from('rival_relationships')
    .select('target_player_id')
    .eq('owner_player_id', playerId)
    .eq('relation_type', 'rival')
  if (error) {
    throw error
  }
  return new Set((data ?? []).map((row) => row.target_player_id))
}

async function getPlayerNames(db: Db, playerIds: string[]): Promise<Map<string, string>> {
  if (playerIds.length === 0) {
    return new Map()
  }
  const { data, error } = await db.from('profiles').select('id, display_name').in('id', playerIds)
  if (error) {
    throw error
  }
  return new Map((data ?? []).map((row) => [row.id, row.display_name || 'Player']))
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
  db: Db,
  playerId: string,
  payload: { evidence?: Record<string, unknown> },
): Promise<'unverified' | 'signed' | 'invalid'> {
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

  const { data: key, error } = await db
    .from('device_keys')
    .select('public_key')
    .eq('id', keyId)
    .eq('player_id', playerId)
    .is('revoked_at', null)
    .maybeSingle()
  if (error || !key) {
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
      Buffer.from(key.public_key, 'hex'),
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
  if (value === null || typeof value !== 'object') {
    return JSON.stringify(value)
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
function playedAtIso(value: unknown): string | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Date(value * 1000).toISOString()
  }
  if (typeof value === 'string' && value.length > 0) {
    return value
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
