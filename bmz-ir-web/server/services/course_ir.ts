import { createHash, randomUUID } from 'node:crypto'
import { and, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../utils/db_errors'
import {
  asRuleMode,
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
import type { IrRuleMode } from '../../shared/types/ir'

const LN_POLICY_ALIASES = new Map([
  ['AutoLn', 'AutoLn'],
  ['AutoCn', 'AutoCn'],
  ['AutoHcn', 'AutoHcn'],
  ['ForceLn', 'ForceLn'],
  ['ForceCn', 'ForceCn'],
  ['ForceHcn', 'ForceHcn'],
  ['auto_ln', 'AutoLn'],
  ['auto_cn', 'AutoCn'],
  ['auto_hcn', 'AutoHcn'],
  ['force_ln', 'ForceLn'],
  ['force_cn', 'ForceCn'],
  ['force_hcn', 'ForceHcn'],
])
const DEVICE_TYPES = new Set(['keyboard', 'controller'])
type CourseDeviceType = 'keyboard' | 'controller'

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
    rule_mode: IrRuleMode
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
  payload.rule.ln_policy = normalizeCourseLnPolicy(payload.rule.ln_policy)
  payload.rule.rule_mode = asRuleMode(payload.rule.rule_mode)
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

function normalizeCourseLnPolicy(value: unknown): string {
  if (typeof value !== 'string') {
    throw new Error('rule.ln_policy is invalid')
  }
  const normalized = LN_POLICY_ALIASES.get(value.trim())
  if (!normalized) {
    throw new Error('rule.ln_policy is invalid')
  }
  return normalized
}

export async function submitCourseScore(
  user: IrRequestUser,
  payload: CourseScoreSubmission,
): Promise<CourseSubmitResponse> {
  await upsertCourse(payload)

  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const verification = await resolveVerification(user.id, payload)

  const deviceType = payload.play_options.device_type as CourseDeviceType
  // best 更新と同じ値を参照するため、DB default に任せずアプリ側で
  // 受信時刻を確定させる。
  const serverReceivedAt = new Date()
  const courseScoreId = randomUUID()
  const insert = {
    id: courseScoreId,
    serverReceivedAt,
    playerId: user.id,
    courseHash: payload.course.course_hash,
    clientName: payload.client.name,
    clientVersion: payload.client.version,
    platform: payload.client.platform,
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
    ruleMode: payload.rule.rule_mode,
    scoring: payload.rule.scoring,
    clearType: payload.result.clear,
    clearRank,
    courseClear: payload.result.course_clear,
    courseFailed: payload.result.course_failed,
    playedEntries: payload.result.played_entries,
    trophies: payload.result.trophies ?? [],
    exScore: payload.result.ex_score,
    maxExScore: payload.result.max_ex_score,
    maxCombo: payload.result.max_combo,
    bp: payload.result.bp,
    judges: payload.result.judges,
    gaugeValue: payload.result.gauge_value,
    entries: payload.result.entries,
    playedAt: playedAtDate(payload.result.played_at),
    deviceType,
    evidence: payload.evidence ?? {},
    verification,
    idempotencyKey: payload.idempotency_key,
  }

  let score: { id: string; serverReceivedAt: Date } = { id: courseScoreId, serverReceivedAt }
  try {
    await db.insert(schema.courseScores).values(insert)
  } catch (error) {
    if (!isUniqueConstraintError(error)) {
      throw error
    }
    // idempotency 重複。既存 score を採用し、best 更新を再試行する。
    const existing = await db.query.courseScores.findFirst({
      columns: { id: true, serverReceivedAt: true },
      where: and(
        eq(schema.courseScores.playerId, user.id),
        eq(schema.courseScores.idempotencyKey, payload.idempotency_key),
      ),
    })
    if (!existing) {
      throw new Error('failed to insert course score')
    }
    score = existing
  }

  // D1 batch 内で直前に作った course_score_id を best_course_scores から
  // 参照すると FK で失敗するため、course score を先に確定保存してから
  // best を更新する。重複 retry 時もここで best の復旧を試みる。
  const bestStatement =
    verification !== 'invalid'
      ? await prepareBestCourseScoreUpsert(user.id, payload, score, clearRank, verification)
      : null
  if (bestStatement) {
    await bestStatement
  }

  if (score.id !== courseScoreId) {
    return {
      accepted: true,
      course_score_id: score.id,
      best_updated: bestStatement !== null,
      server_received_at: score.serverReceivedAt.toISOString(),
    }
  }

  return {
    accepted: true,
    course_score_id: courseScoreId,
    best_updated: bestStatement !== null,
    server_received_at: serverReceivedAt.toISOString(),
  }
}

async function upsertCourse(payload: CourseScoreSubmission) {
  const values = {
    courseHash: payload.course.course_hash,
    title: payload.course.title ?? '',
    kind: payload.course.kind ?? 'course',
    charts: payload.course.charts,
    chartCount: payload.course.charts.length,
    constraints: payload.course.constraints ?? {},
    sourceUrl: payload.course.source_url ?? null,
    updatedAt: new Date(),
  }
  await db
    .insert(schema.irCourses)
    .values(values)
    .onConflictDoUpdate({ target: schema.irCourses.courseHash, set: values })
}

/**
 * best_course_scores 更新の要否を判定し、必要なら未実行の upsert statement を
 * 返す。course score insert の確定後に単体実行する。
 */
async function prepareBestCourseScoreUpsert(
  playerId: string,
  payload: CourseScoreSubmission,
  score: { id: string; serverReceivedAt: Date },
  clearRank: number,
  verification: string,
) {
  const current = await db.query.bestCourseScores.findFirst({
    columns: { exScore: true, clearRank: true, bp: true, maxCombo: true },
    where: and(
      eq(schema.bestCourseScores.playerId, playerId),
      eq(schema.bestCourseScores.courseHash, payload.course.course_hash),
      eq(schema.bestCourseScores.gauge, payload.rule.gauge),
      eq(schema.bestCourseScores.lnPolicy, payload.rule.ln_policy),
      eq(schema.bestCourseScores.ruleMode, payload.rule.rule_mode),
      eq(schema.bestCourseScores.scoring, payload.rule.scoring),
    ),
  })

  const next = {
    exScore: payload.result.ex_score,
    clearRank,
    bp: payload.result.bp,
    maxCombo: payload.result.max_combo,
  }
  const wins =
    !current ||
    next.exScore > current.exScore ||
    (next.exScore === current.exScore && next.clearRank > current.clearRank) ||
    (next.exScore === current.exScore &&
      next.clearRank === current.clearRank &&
      next.bp < current.bp) ||
    (next.exScore === current.exScore &&
      next.clearRank === current.clearRank &&
      next.bp === current.bp &&
      next.maxCombo > current.maxCombo)
  if (!wins) {
    return null
  }

  const values = {
    id: randomUUID(),
    playerId,
    courseHash: payload.course.course_hash,
    courseScoreId: score.id,
    exScore: payload.result.ex_score,
    clearType: payload.result.clear,
    clearRank,
    courseClear: payload.result.course_clear,
    maxCombo: payload.result.max_combo,
    bp: payload.result.bp,
    deviceType: payload.play_options.device_type as CourseDeviceType,
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
    ruleMode: payload.rule.rule_mode,
    scoring: payload.rule.scoring,
    playedAt: playedAtDate(payload.result.played_at),
    serverReceivedAt: score.serverReceivedAt,
    verification: verification as 'unverified' | 'signed' | 'invalid' | 'trusted',
  }
  return db
    .insert(schema.bestCourseScores)
    .values(values)
    .onConflictDoUpdate({
      target: [
        schema.bestCourseScores.playerId,
        schema.bestCourseScores.courseHash,
        schema.bestCourseScores.gauge,
        schema.bestCourseScores.lnPolicy,
        schema.bestCourseScores.ruleMode,
        schema.bestCourseScores.scoring,
      ],
      set: {
        courseScoreId: values.courseScoreId,
        exScore: values.exScore,
        clearType: values.clearType,
        clearRank: values.clearRank,
        courseClear: values.courseClear,
        maxCombo: values.maxCombo,
        bp: values.bp,
        deviceType: values.deviceType,
        playedAt: values.playedAt,
        serverReceivedAt: values.serverReceivedAt,
        verification: values.verification,
      },
    })
}

function playedAtDate(value: unknown): Date | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return new Date(value * 1000)
  }
  if (typeof value === 'string' && value.length > 0) {
    return new Date(value)
  }
  return null
}
