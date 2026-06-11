import { createHash } from 'node:crypto'
import type { SupabaseClient } from '@supabase/supabase-js'
import type { Database, Json } from '../../shared/types/database.types'
import {
  CLEAR_RANK,
  isRecord,
  normalizeGaugeName,
  requireHex,
  requireFiniteNumber,
  requireNonNegativeInteger,
  resolveVerification,
  stableStringify,
  type IrRequestUser,
} from './ir'

type Db = SupabaseClient<Database>

const LN_POLICIES = new Set(['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn'])
const DEVICE_TYPES = new Set(['keyboard', 'controller'])

export interface CourseScoreSubmission {
  client: { name: string; version: string; platform: string }
  course: {
    course_hash: string
    title?: string
    kind?: 'dan' | 'course'
    charts: string[]
    constraints?: Record<string, unknown>
    source_url?: string | null
  }
  rule: {
    gauge: string
    /** LnPolicySetting 値 (コースは譜面ごとに解決が変わるため設定値)。 */
    ln_policy: string
    scoring: 'bms_ex_score_v1'
  }
  result: {
    clear: string
    course_clear: boolean
    course_failed: boolean
    played_entries: number
    trophies?: string[]
    ex_score: number
    max_ex_score: number
    max_combo: number
    bp: number
    judges: Record<string, unknown>
    gauge_value: number
    entries: Record<string, unknown>[]
    played_at?: string | number | null
  }
  play_options: { device_type: string } & Record<string, unknown>
  evidence?: Record<string, unknown>
  idempotency_key: string
}

export interface CourseSubmitResponse {
  accepted: boolean
  course_score_id: string
  best_updated: boolean
  server_received_at: string
}

/** course identity: 譜面 sha256 リスト + constraints の canonical JSON の SHA256。 */
export function computeCourseHash(charts: string[], constraints: Record<string, unknown>): string {
  const canonical = stableStringify({ charts, constraints })
  return createHash('sha256').update(canonical).digest('hex')
}

export function validateCourseScoreSubmission(value: unknown): CourseScoreSubmission {
  if (!isRecord(value)) {
    throw new Error('payload must be an object')
  }
  const payload = value as unknown as CourseScoreSubmission
  if (!isRecord(payload.client) || !isRecord(payload.course) || !isRecord(payload.rule)) {
    throw new Error('client, course, and rule are required')
  }
  if (!isRecord(payload.result)) {
    throw new Error('result is required')
  }
  requireHex(payload.course.course_hash, 64, 'course.course_hash')
  if (!Array.isArray(payload.course.charts) || payload.course.charts.length === 0) {
    throw new Error('course.charts must be a non-empty array')
  }
  for (const sha of payload.course.charts) {
    requireHex(sha, 64, 'course.charts[]')
  }
  // identity の自己申告が定義と一致することを検証する。
  const expected = computeCourseHash(payload.course.charts, payload.course.constraints ?? {})
  if (expected !== payload.course.course_hash) {
    throw new Error('course.course_hash does not match the course definition')
  }
  if (!LN_POLICIES.has(payload.rule.ln_policy)) {
    throw new Error('rule.ln_policy is invalid')
  }
  if (payload.rule.scoring !== 'bms_ex_score_v1') {
    throw new Error('rule.scoring is unsupported')
  }
  payload.rule.gauge = normalizeGaugeName(payload.rule.gauge)
  for (const field of ['ex_score', 'max_ex_score', 'max_combo', 'bp', 'played_entries'] as const) {
    requireNonNegativeInteger(payload.result[field], `result.${field}`)
  }
  requireFiniteNumber(payload.result.gauge_value, 'result.gauge_value')
  if (!isRecord(payload.result.judges)) {
    throw new Error('result.judges must be an object')
  }
  if (!Array.isArray(payload.result.entries)) {
    throw new Error('result.entries must be an array')
  }
  for (const [index, entry] of payload.result.entries.entries()) {
    if (!isRecord(entry)) {
      throw new Error(`result.entries[${index}] must be an object`)
    }
  }
  if (!payload.idempotency_key || typeof payload.idempotency_key !== 'string') {
    throw new Error('idempotency_key is required')
  }
  if (
    !isRecord(payload.play_options) ||
    !DEVICE_TYPES.has(String(payload.play_options.device_type))
  ) {
    throw new Error('play_options.device_type is invalid')
  }
  return payload
}

export async function submitCourseScore(
  db: Db,
  user: IrRequestUser,
  payload: CourseScoreSubmission,
): Promise<CourseSubmitResponse> {
  await upsertCourse(db, payload)

  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const verification = await resolveVerification(db, user.id, payload)

  const insert = {
    player_id: user.id,
    course_hash: payload.course.course_hash,
    client_name: payload.client.name,
    client_version: payload.client.version,
    platform: payload.client.platform,
    gauge: payload.rule.gauge,
    ln_policy: payload.rule.ln_policy,
    scoring: payload.rule.scoring,
    clear_type: payload.result.clear,
    clear_rank: clearRank,
    course_clear: payload.result.course_clear,
    course_failed: payload.result.course_failed,
    played_entries: payload.result.played_entries,
    trophies: (payload.result.trophies ?? []) as Json,
    ex_score: payload.result.ex_score,
    max_ex_score: payload.result.max_ex_score,
    max_combo: payload.result.max_combo,
    bp: payload.result.bp,
    judges: payload.result.judges as Json,
    gauge_value: payload.result.gauge_value,
    entries: payload.result.entries as Json,
    played_at: playedAtIso(payload.result.played_at),
    device_type: payload.play_options.device_type,
    evidence: (payload.evidence ?? {}) as Json,
    verification,
    idempotency_key: payload.idempotency_key,
  }

  const { data: inserted, error: insertError } = await db
    .from('course_scores')
    .insert(insert)
    .select('id, server_received_at')
    .single()
  if (insertError) {
    const { data: existing, error: existingError } = await db
      .from('course_scores')
      .select('id, server_received_at')
      .eq('player_id', user.id)
      .eq('idempotency_key', payload.idempotency_key)
      .maybeSingle()
    if (existingError || !existing) {
      throw insertError
    }
    return {
      accepted: true,
      course_score_id: existing.id,
      best_updated: false,
      server_received_at: existing.server_received_at,
    }
  }
  const score = inserted
  if (!score) {
    throw insertError ?? new Error('failed to insert course score')
  }

  // invalid (署名不正) は単曲と同じく best 更新の対象外。
  let bestUpdated = false
  if (verification !== 'invalid') {
    bestUpdated = await upsertBestCourseScore(db, user.id, payload, score, clearRank, verification)
  }

  return {
    accepted: true,
    course_score_id: score.id,
    best_updated: bestUpdated,
    server_received_at: score.server_received_at,
  }
}

async function upsertCourse(db: Db, payload: CourseScoreSubmission) {
  const { error } = await db.from('ir_courses').upsert(
    {
      course_hash: payload.course.course_hash,
      title: payload.course.title ?? '',
      kind: payload.course.kind ?? 'course',
      charts: payload.course.charts as unknown as Json,
      chart_count: payload.course.charts.length,
      constraints: (payload.course.constraints ?? {}) as Json,
      source_url: payload.course.source_url ?? null,
    },
    { onConflict: 'course_hash' },
  )
  if (error) {
    throw error
  }
}

async function upsertBestCourseScore(
  db: Db,
  playerId: string,
  payload: CourseScoreSubmission,
  score: { id: string; server_received_at: string },
  clearRank: number,
  verification: string,
): Promise<boolean> {
  const { data: current, error: currentError } = await db
    .from('best_course_scores')
    .select('ex_score, clear_rank, bp, max_combo')
    .eq('player_id', playerId)
    .eq('course_hash', payload.course.course_hash)
    .eq('gauge', payload.rule.gauge)
    .eq('ln_policy', payload.rule.ln_policy)
    .eq('scoring', payload.rule.scoring)
    .maybeSingle()
  if (currentError) {
    throw currentError
  }

  const next = {
    ex_score: payload.result.ex_score,
    clear_rank: clearRank,
    bp: payload.result.bp,
    max_combo: payload.result.max_combo,
  }
  const wins =
    !current ||
    next.ex_score > current.ex_score ||
    (next.ex_score === current.ex_score && next.clear_rank > current.clear_rank) ||
    (next.ex_score === current.ex_score &&
      next.clear_rank === current.clear_rank &&
      next.bp < current.bp) ||
    (next.ex_score === current.ex_score &&
      next.clear_rank === current.clear_rank &&
      next.bp === current.bp &&
      next.max_combo > current.max_combo)
  if (!wins) {
    return false
  }

  const { error } = await db.from('best_course_scores').upsert(
    {
      player_id: playerId,
      course_hash: payload.course.course_hash,
      course_score_id: score.id,
      ex_score: payload.result.ex_score,
      clear_type: payload.result.clear,
      clear_rank: clearRank,
      course_clear: payload.result.course_clear,
      max_combo: payload.result.max_combo,
      bp: payload.result.bp,
      device_type: payload.play_options.device_type,
      gauge: payload.rule.gauge,
      ln_policy: payload.rule.ln_policy,
      scoring: payload.rule.scoring,
      played_at: playedAtIso(payload.result.played_at),
      server_received_at: score.server_received_at,
      verification,
    },
    { onConflict: 'player_id,course_hash,gauge,ln_policy,scoring' },
  )
  if (error) {
    throw error
  }
  return true
}

function playedAtIso(value: unknown): string | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Date(value * 1000).toISOString()
  }
  if (typeof value === 'string' && value.length > 0) {
    return value
  }
  return null
}
