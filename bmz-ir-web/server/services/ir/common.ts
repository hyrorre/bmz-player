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
  MAX_EVIDENCE_BYTES,
  requireJsonBytes,
  validateChartInputLimits,
  validateSubmissionJsonLimits,
} from './input_limits'

export const LN_POLICIES = new Set([
  'AutoLn',
  'AutoCn',
  'AutoHcn',
  'ForceLn',
  'ForceCn',
  'ForceHcn',
])
export const EFFECTIVE_LN_MODES = new Set(['ln', 'cn', 'hcn'])
export const DEVICE_TYPES = new Set(['keyboard', 'controller'])
export const RULE_MODES = new Set(['Beatoraja', 'Lr2Oraja', 'Dx'])
export const RANKING_SCOPES = new Set([
  'global',
  'self_and_rivals',
  'rivals',
  'self',
  'around_self',
])
export const LOCAL_BACKFILL_SOURCE = 'local_backfill'
// D1 は 1 query あたり最大100 bind parameter。best score 再集計は score key ごとに
// 5 bind を使うため、player / accepted 条件を含めても余裕を持てる19件に制限する。
export const MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE = 19
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
  ruleMode?: IrRuleMode
  scoring: 'bms_ex_score_v1'
}

export interface BestScoreCandidate {
  ex_score: number
  clear_rank: number
  max_combo: number
  min_bp: number
  min_cb: number
  server_received_at: Date
}

export function bestCandidateWins(next: BestScoreCandidate, current: BestScoreCandidate): boolean {
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

export interface BestScoreRow extends BestScoreCandidate {
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
  arrange_1p?: string
  arrange_2p?: string
  played_at: string | null
  verification: IrVerificationStatus
  judges?: IrJudges
}

export class IrEvidenceValidationError extends Error {}
export class IrScoreNotFoundError extends Error {}
export class IrBackfillCleanupError extends Error {}

export interface ScoreAttestationPayload {
  score_id: string
  purpose: 'score_attestation'
  evidence: Record<string, unknown>
}

export interface ScoreHistoryRankingRow extends Omit<BestScoreRow, 'score_id'> {
  id: string
}

export interface ScoreSubmissionMetadata {
  doubleOption: IrDoubleOption
  appliedDoubleOption: IrAppliedDoubleOption
  sourceKind: IrScoreSourceKind
}

export interface BestScoreKey {
  chartSha256: string
  lnPolicy: LnScorePolicy
  doubleOption: IrDoubleOption
  ruleMode: IrRuleMode
  scoring: 'bms_ex_score_v1'
}

export interface LocalBackfillDeleteResult {
  deleted_score_ids: string[]
  missing_score_ids: string[]
  retained_score_ids: string[]
}

export function parseRankingQuery(query: Record<string, unknown>): RankingQuery {
  const scope = asScope(String(query.scope ?? 'global'))
  const limit = clampInteger(query.limit, 100, 1, 200)
  const offset = clampInteger(query.offset, 0, 0, 100_000)
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy ? asLnPolicy(query.ln_policy) : undefined
  const doubleOption = normalizeDoubleOption(query.double_option)
  const ruleMode =
    typeof query.rule_mode === 'string' && query.rule_mode && query.rule_mode !== 'ALL'
      ? asRuleMode(query.rule_mode)
      : undefined
  const scoring = String(query.scoring ?? 'bms_ex_score_v1')
  if (scoring !== 'bms_ex_score_v1') {
    throw new Error('unsupported scoring')
  }
  return { scope, limit, offset, lnPolicy, doubleOption, ruleMode, scoring }
}

export function parseRankingScope(value: string): IrRankingScope {
  return asScope(value)
}

export function arrangeOptionsFromPlayOptions(
  playOptions: Record<string, unknown> | null | undefined,
): { arrange_1p?: string; arrange_2p?: string } {
  const legacyOption =
    typeof playOptions?.option === 'string' && playOptions.option.length > 0
      ? playOptions.option
      : undefined
  return {
    arrange_1p:
      typeof playOptions?.arrange_1p === 'string' && playOptions.arrange_1p.length > 0
        ? playOptions.arrange_1p
        : legacyOption,
    arrange_2p:
      typeof playOptions?.arrange_2p === 'string' && playOptions.arrange_2p.length > 0
        ? playOptions.arrange_2p
        : undefined,
  }
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
  validateChartInputLimits(payload.chart)
  validateSubmissionJsonLimits(payload.play_options, payload.evidence)
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
  const result = payload.result
  if (result.ex_score / 2 > result.notes || result.max_combo > result.notes) {
    throw new Error('result score or combo exceeds result.notes')
  }
  if (result.pass_notes != null && result.pass_notes > result.notes) {
    throw new Error('result.pass_notes exceeds result.notes')
  }
  // LR2 stores empty POOR in poor, so it is not a count of consumed notes.
  const isLr2 = normalizeScoreSourceKind(payload.play_options?.source_kind) === 'lr2'
  const noteJudges = isLr2
    ? ['pgreat', 'great', 'good', 'bad']
    : ['pgreat', 'great', 'good', 'bad', 'poor']
  const judgedNotes = noteJudges.reduce(
    (sum, key) => sum + judgeTotal(payload, key as keyof typeof result.judges.fast),
    0,
  )
  // Failed plays can leave notes unjudged; empty POOR does not consume a note.
  if (!Number.isSafeInteger(judgedNotes) || judgedNotes > result.notes) {
    throw new Error('result.judges exceeds result.notes')
  }
  const expectedExScore = 2 * judgeTotal(payload, 'pgreat') + judgeTotal(payload, 'great')
  if (!Number.isSafeInteger(expectedExScore) || result.ex_score !== expectedExScore) {
    throw new Error('result.ex_score does not match result.judges')
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
  validateSeedOptions(payload.play_options)
  scoreSubmissionMetadata(payload.play_options)
  return payload
}

export function validateSeedOptions(playOptions: Record<string, unknown>) {
  for (const key of ['seed', 'random_seed'] as const) {
    const value = playOptions[key]
    if (value === undefined || value === null) continue
    if (typeof value === 'number' && Number.isSafeInteger(value)) continue
    if (typeof value === 'string' && /^-?\d+$/.test(value)) {
      const seed = BigInt(value)
      if (seed >= -(1n << 63n) && seed <= (1n << 63n) - 1n) continue
    }
    throw new Error(`play_options.${key} is invalid`)
  }
}

export function validateScoreAttestation(value: unknown): ScoreAttestationPayload {
  if (!isRecord(value)) {
    throw new IrEvidenceValidationError('score attestation payload must be an object')
  }
  if (typeof value.score_id !== 'string' || value.score_id.length === 0) {
    throw new IrEvidenceValidationError('score_id is required')
  }
  if (value.purpose !== 'score_attestation') {
    throw new IrEvidenceValidationError('score attestation purpose is invalid')
  }
  if (!isRecord(value.evidence)) {
    throw new IrEvidenceValidationError('score attestation evidence is required')
  }
  try {
    requireJsonBytes(value.evidence, MAX_EVIDENCE_BYTES, 'evidence')
  } catch (error) {
    throw new IrEvidenceValidationError(error instanceof Error ? error.message : 'invalid evidence')
  }
  return {
    score_id: value.score_id,
    purpose: value.purpose,
    evidence: value.evidence,
  }
}

export function playedAtDate(value: unknown): Date | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Date(value * 1000)
  }
  if (typeof value === 'string' && value.length > 0) {
    return new Date(value)
  }
  return null
}

export function judgeTotal(
  payload: IrScoreSubmission,
  key: keyof IrScoreSubmission['result']['judges']['fast'],
): number {
  return payload.result.judges.fast[key] + payload.result.judges.slow[key]
}

export function asLnPolicy(value: string): LnScorePolicy {
  if (!LN_POLICIES.has(value)) {
    throw new Error('ln_policy is invalid')
  }
  return value as LnScorePolicy
}

export function asRuleMode(value: unknown): IrRuleMode {
  if (typeof value !== 'string' || !RULE_MODES.has(value)) {
    throw new Error('rule_mode is invalid')
  }
  return value as IrRuleMode
}

export function asScope(value: string): IrRankingScope {
  if (!RANKING_SCOPES.has(value)) {
    throw new Error('scope is invalid')
  }
  return value as IrRankingScope
}

export function normalizeDoubleOption(value: unknown): IrDoubleOption {
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

export function scoreSubmissionMetadata(
  playOptions: IrScoreSubmission['play_options'],
): ScoreSubmissionMetadata {
  const doubleOption = normalizeDoubleOption(playOptions.double_option)
  const appliedDoubleOption = normalizeAppliedDoubleOption(
    playOptions.applied_double_option,
    doubleOption,
  )
  if (normalizeDoubleOption(appliedDoubleOption) !== doubleOption) {
    throw new Error('applied_double_option does not match double_option')
  }
  return {
    doubleOption,
    appliedDoubleOption,
    sourceKind: normalizeScoreSourceKind(playOptions.source_kind),
  }
}

export function normalizeAppliedDoubleOption(
  value: unknown,
  fallback: IrDoubleOption,
): IrAppliedDoubleOption {
  const normalized = String(value ?? fallback)
    .trim()
    .toLowerCase()
    .replaceAll('-', '_')
  switch (normalized) {
    case '':
      return fallback
    case 'off':
      return 'off'
    case 'flip':
      return 'flip'
    case 'battle':
      return 'battle'
    case 'battle_auto_scratch':
    case 'battle_assist':
      return 'battle_auto_scratch'
    default:
      throw new Error('applied_double_option is invalid')
  }
}

export function normalizeScoreSourceKind(value: unknown): IrScoreSourceKind {
  const normalized = String(value ?? 'local')
    .trim()
    .toLowerCase()
    .replaceAll('-', '_')
  switch (normalized) {
    case '':
    case 'local':
      return 'local'
    case 'beatoraja':
      return 'beatoraja'
    case 'lr2':
      return 'lr2'
    case 'lr2oraja':
    case 'lr2_oraja':
      return 'lr2oraja'
    case 'lr2oraja_dx':
    case 'lr2_oraja_dx':
      return 'lr2oraja_dx'
    default:
      throw new Error('source_kind is invalid')
  }
}

export function clampInteger(value: unknown, fallback: number, min: number, max: number): number {
  const parsed = Number(value ?? fallback)
  if (!Number.isFinite(parsed)) {
    return fallback
  }
  return Math.max(min, Math.min(max, Math.trunc(parsed)))
}

export function nonEmptyString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.length > 0 ? value : fallback
}

export function requireHex(value: unknown, length: number, label: string) {
  if (typeof value !== 'string' || !new RegExp(`^[0-9a-f]{${length}}$`).test(value)) {
    throw new Error(`${label} must be lowercase hex length ${length}`)
  }
}

export function requireNonNegativeInteger(value: unknown, label: string) {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new Error(`${label} must be a non-negative safe integer`)
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
