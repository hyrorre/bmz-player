import { createHash, randomUUID } from 'node:crypto'
import { and, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { isUniqueConstraintError } from '../utils/db_errors'
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

const LN_POLICIES = new Set(['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn'])
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
  user: IrRequestUser,
  payload: CourseScoreSubmission,
): Promise<CourseSubmitResponse> {
  await upsertCourse(payload)

  const clearRank = CLEAR_RANK[payload.result.clear] ?? 0
  const verification = await resolveVerification(user.id, payload)

  const deviceType = payload.play_options.device_type as CourseDeviceType
  const insert = {
    id: randomUUID(),
    playerId: user.id,
    courseHash: payload.course.course_hash,
    clientName: payload.client.name,
    clientVersion: payload.client.version,
    platform: payload.client.platform,
    gauge: payload.rule.gauge,
    lnPolicy: payload.rule.ln_policy,
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

  const inserted = await insertCourseScore(insert)
  if (!inserted) {
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
    return {
      accepted: true,
      course_score_id: existing.id,
      best_updated: false,
      server_received_at: existing.serverReceivedAt.toISOString(),
    }
  }

  // invalid (署名不正) は単曲と同じく best 更新の対象外。
  let bestUpdated = false
  if (verification !== 'invalid') {
    bestUpdated = await upsertBestCourseScore(user.id, payload, inserted, clearRank, verification)
  }

  return {
    accepted: true,
    course_score_id: inserted.id,
    best_updated: bestUpdated,
    server_received_at: inserted.serverReceivedAt.toISOString(),
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

async function upsertBestCourseScore(
  playerId: string,
  payload: CourseScoreSubmission,
  score: { id: string; serverReceivedAt: Date },
  clearRank: number,
  verification: string,
): Promise<boolean> {
  const current = await db.query.bestCourseScores.findFirst({
    columns: { exScore: true, clearRank: true, bp: true, maxCombo: true },
    where: and(
      eq(schema.bestCourseScores.playerId, playerId),
      eq(schema.bestCourseScores.courseHash, payload.course.course_hash),
      eq(schema.bestCourseScores.gauge, payload.rule.gauge),
      eq(schema.bestCourseScores.lnPolicy, payload.rule.ln_policy),
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
    return false
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
    scoring: payload.rule.scoring,
    playedAt: playedAtDate(payload.result.played_at),
    serverReceivedAt: score.serverReceivedAt,
    verification: verification as 'unverified' | 'signed' | 'invalid' | 'trusted',
  }
  await db
    .insert(schema.bestCourseScores)
    .values(values)
    .onConflictDoUpdate({
      target: [
        schema.bestCourseScores.playerId,
        schema.bestCourseScores.courseHash,
        schema.bestCourseScores.gauge,
        schema.bestCourseScores.lnPolicy,
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
  return true
}

async function insertCourseScore(values: typeof schema.courseScores.$inferInsert) {
  try {
    const [inserted] = await db.insert(schema.courseScores).values(values).returning({
      id: schema.courseScores.id,
      serverReceivedAt: schema.courseScores.serverReceivedAt,
    })
    return inserted ?? null
  } catch (error) {
    if (isUniqueConstraintError(error)) {
      return null
    }
    throw error
  }
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
