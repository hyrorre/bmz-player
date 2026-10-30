import { randomUUID } from 'node:crypto'
import {
  and,
  count,
  desc,
  eq,
  inArray,
  isNull,
  ne,
  notExists,
  or,
  sql,
  type like,
  type SQL,
} from 'drizzle-orm'
import { alias } from 'drizzle-orm/sqlite-core'
import type { BatchItem } from 'drizzle-orm/batch'
import { db, schema } from 'hub:db'
import { banSessionStatements } from '../../utils/auth_tokens'
import {
  CHART_MODERATION_STATUSES,
  MAX_CHART_MODERATION_BATCH_SIZE,
  MAX_MODERATION_NOTE_LENGTH,
  MAX_MODERATION_REASON_LENGTH,
  USER_MODERATION_STATUSES,
  type ChartModerationStatus,
  type ModerationAction,
  type UserModerationStatus,
} from '../../../shared/constants/moderation'

/** 管理者APIの入力・権限・競合エラー。handler で同じ status code に変換する。 */
export class ModerationAdminError extends Error {
  constructor(
    readonly statusCode: 400 | 403 | 404 | 409,
    message: string,
    readonly data?: Record<string, unknown>,
  ) {
    super(message)
  }
}

const USER_ACTIONS: Record<UserModerationStatus, ModerationAction> = {
  active: 'user_restore',
  submit_suspended: 'user_suspend',
  banned: 'user_ban',
}

const CHART_ACTIONS: Record<ChartModerationStatus, ModerationAction> = {
  active: 'chart_restore',
  under_review: 'chart_review',
  hidden: 'chart_hide',
  blocked: 'chart_block',
}

export const MAX_ADMIN_SEARCH_LENGTH = 200
const MAX_ADMIN_PAGE_SIZE = 100

// ---------------------------------------------------------------------------
// 入力検証
// ---------------------------------------------------------------------------

export function parseUserModerationStatus(value: unknown): UserModerationStatus {
  if (
    typeof value !== 'string' ||
    !(USER_MODERATION_STATUSES as readonly string[]).includes(value)
  ) {
    throw new ModerationAdminError(400, 'status is invalid')
  }
  return value as UserModerationStatus
}

export function parseChartModerationStatus(value: unknown): ChartModerationStatus {
  if (
    typeof value !== 'string' ||
    !(CHART_MODERATION_STATUSES as readonly string[]).includes(value)
  ) {
    throw new ModerationAdminError(400, 'status is invalid')
  }
  return value as ChartModerationStatus
}

export function parseReason(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) {
    throw new ModerationAdminError(400, 'reason is required')
  }
  const reason = value.trim()
  if (reason.length > MAX_MODERATION_REASON_LENGTH) {
    throw new ModerationAdminError(
      400,
      `reason must be at most ${MAX_MODERATION_REASON_LENGTH} characters`,
    )
  }
  return reason
}

export function parseNote(value: unknown): string | undefined {
  if (value === undefined || value === null) {
    return undefined
  }
  if (typeof value !== 'string') {
    throw new ModerationAdminError(400, 'note must be a string')
  }
  const note = value.trim()
  if (note.length > MAX_MODERATION_NOTE_LENGTH) {
    throw new ModerationAdminError(
      400,
      `note must be at most ${MAX_MODERATION_NOTE_LENGTH} characters`,
    )
  }
  return note
}

export function parseExpectedRevision(value: unknown, label = 'expected_revision'): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new ModerationAdminError(400, `${label} must be a non-negative integer`)
  }
  return Number(value)
}

export function parseChartSha256(value: unknown, label = 'sha256'): string {
  const sha256 = typeof value === 'string' ? value.trim().toLowerCase() : ''
  if (!/^[0-9a-f]{64}$/.test(sha256)) {
    throw new ModerationAdminError(400, `${label} must be hex length 64`)
  }
  return sha256
}

export function parseChartMd5(value: unknown): string {
  const md5 = typeof value === 'string' ? value.trim().toLowerCase() : ''
  if (!/^[0-9a-f]{32}$/.test(md5)) {
    throw new ModerationAdminError(400, 'md5 must be hex length 32')
  }
  return md5
}

export interface ChartModerationItem {
  sha256: string
  expectedRevision: number
}

export function parseChartModerationItems(value: unknown): ChartModerationItem[] {
  if (!Array.isArray(value) || value.length === 0) {
    throw new ModerationAdminError(400, 'items must be a non-empty array')
  }
  if (value.length > MAX_CHART_MODERATION_BATCH_SIZE) {
    throw new ModerationAdminError(
      400,
      `items must contain at most ${MAX_CHART_MODERATION_BATCH_SIZE} charts`,
    )
  }
  const seen = new Set<string>()
  return value.map((item, index) => {
    if (typeof item !== 'object' || item === null || Array.isArray(item)) {
      throw new ModerationAdminError(400, `items[${index}] must be an object`)
    }
    const record = item as Record<string, unknown>
    const sha256 = parseChartSha256(record.sha256, `items[${index}].sha256`)
    if (seen.has(sha256)) {
      throw new ModerationAdminError(400, 'items must not contain duplicate charts')
    }
    seen.add(sha256)
    return {
      sha256,
      expectedRevision: parseExpectedRevision(
        record.expected_revision,
        `items[${index}].expected_revision`,
      ),
    }
  })
}

export function parsePage(query: Record<string, unknown>, defaultLimit = 50) {
  const limit = clamp(query.limit, defaultLimit, 1, MAX_ADMIN_PAGE_SIZE)
  const offset = clamp(query.offset, 0, 0, 100_000)
  return { limit, offset }
}

export function parseSearch(value: unknown): string {
  if (value === undefined || value === null) return ''
  if (typeof value !== 'string') {
    throw new ModerationAdminError(400, 'search must be a single value')
  }
  const search = value.trim()
  if (search.length > MAX_ADMIN_SEARCH_LENGTH) {
    throw new ModerationAdminError(
      400,
      `search must be at most ${MAX_ADMIN_SEARCH_LENGTH} characters`,
    )
  }
  return search
}

function clamp(value: unknown, fallback: number, min: number, max: number) {
  const parsed = Number(value ?? fallback)
  if (!Number.isFinite(parsed)) return fallback
  return Math.max(min, Math.min(max, Math.trunc(parsed)))
}

function likePattern(value: string) {
  return `%${value.replace(/[\\%_]/g, (match) => `\\${match}`)}%`
}

function likeEscaped(column: Parameters<typeof like>[0], value: string): SQL {
  return sql`${column} like ${likePattern(value)} escape '\\'`
}

// ---------------------------------------------------------------------------
// 状態変更
// ---------------------------------------------------------------------------

export interface UserModerationChangeInput {
  actorId: string
  adminUserIds: Set<string>
  userId: string
  status: UserModerationStatus
  reason: string
  expectedRevision: number
}

export interface ModerationChangeResult<TStatus extends string> {
  /** changed: 変更して監査ログを記録した。unchanged: すでに同じ状態。conflict: 他の変更と衝突。 */
  result: 'changed' | 'unchanged' | 'conflict'
  status: TStatus
  revision: number
  audit_log_id: string | null
}

/**
 * ユーザーの利用状態を変更する。
 *
 * 状態の upsert、監査ログ、BAN 時のセッション失効を1つの D1 batch (暗黙transaction) で書く。
 * 監査ログとセッション失効は「この変更で last_change_id が更新された場合だけ」実行する条件付き
 * SQL にして、楽観的ロックに負けた変更がログやセッションへ影響しないようにする。
 */
export async function changeUserModeration(
  input: UserModerationChangeInput,
): Promise<ModerationChangeResult<UserModerationStatus>> {
  if (input.userId === input.actorId) {
    throw new ModerationAdminError(403, 'Administrators cannot change their own account status')
  }
  if (input.status !== 'active' && input.adminUserIds.has(input.userId)) {
    throw new ModerationAdminError(403, 'Administrator accounts cannot be restricted')
  }
  const users = await db
    .select({ id: schema.users.id })
    .from(schema.users)
    .where(eq(schema.users.id, input.userId))
    .limit(1)
  if (users.length === 0) {
    throw new ModerationAdminError(404, 'Player not found')
  }

  const current = await readUserModeration(input.userId)
  if (current.revision !== input.expectedRevision) {
    return {
      result: 'conflict',
      status: current.status,
      revision: current.revision,
      audit_log_id: null,
    }
  }
  if (current.status === input.status) {
    return {
      result: 'unchanged',
      status: current.status,
      revision: current.revision,
      audit_log_id: null,
    }
  }

  const changeId = randomUUID()
  const now = new Date()
  const table = schema.userModeration
  const values = {
    userId: input.userId,
    status: input.status,
    reason: input.reason,
    lastChangeId: changeId,
    updatedBy: input.actorId,
    updatedAt: now,
  }
  const stateStatement =
    current.revision === 0
      ? db
          .insert(table)
          .values({ ...values, revision: 1 })
          .onConflictDoNothing()
      : db
          .update(table)
          .set({ ...values, revision: sql`${table.revision} + 1` })
          .where(and(eq(table.userId, input.userId), eq(table.revision, input.expectedRevision)))
  const appliedByThisChange = sql`exists (select 1 from ${table} where ${table.userId} = ${input.userId} and ${table.lastChangeId} = ${changeId})`
  const statements: [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]] = [
    stateStatement,
    auditInsertStatement({
      id: changeId,
      actorUserId: input.actorId,
      targetType: 'user',
      targetId: input.userId,
      action: USER_ACTIONS[input.status],
      previousStatus: current.status,
      newStatus: input.status,
      reason: input.reason,
      metadata: { revision: current.revision + 1, revoked_sessions: input.status === 'banned' },
      createdAt: now,
      condition: appliedByThisChange,
    }),
  ]
  if (input.status === 'banned') {
    statements.push(...banSessionStatements(input.userId, now, appliedByThisChange))
  }
  await db.batch(statements)

  const after = await readUserModeration(input.userId)
  if (after.lastChangeId !== changeId) {
    return {
      result: 'conflict',
      status: after.status,
      revision: after.revision,
      audit_log_id: null,
    }
  }
  return {
    result: 'changed',
    status: after.status,
    revision: after.revision,
    audit_log_id: changeId,
  }
}

async function readUserModeration(userId: string) {
  const rows = await db
    .select({
      status: schema.userModeration.status,
      revision: schema.userModeration.revision,
      lastChangeId: schema.userModeration.lastChangeId,
    })
    .from(schema.userModeration)
    .where(eq(schema.userModeration.userId, userId))
    .limit(1)
  return rows[0] ?? { status: 'active' as const, revision: 0, lastChangeId: null }
}

export interface ChartModerationChangeInput {
  actorId: string
  items: ChartModerationItem[]
  status: ChartModerationStatus
  reason: string
  note?: string
}

export interface ChartModerationChangeResult extends ModerationChangeResult<ChartModerationStatus> {
  sha256: string
}

/**
 * 譜面の状態を1件以上まとめて変更する。
 *
 * 1リクエストの対象は MAX_CHART_MODERATION_BATCH_SIZE 件までで、全件を1つの batch で書く。
 * 譜面ごとに revision を照合し、衝突した譜面だけ conflict として返す (部分成功)。
 * 同じリクエストを再送しても、適用済みの譜面は unchanged になり監査ログは増えない。
 * MD5 や title が一致する別の SHA-256 へは広げず、指定された SHA-256 だけを変更する。
 */
export async function changeChartModeration(
  input: ChartModerationChangeInput,
): Promise<ChartModerationChangeResult[]> {
  const shaList = input.items.map((item) => item.sha256)
  const currentRows = await db
    .select({
      sha256: schema.chartModeration.chartSha256,
      status: schema.chartModeration.status,
      revision: schema.chartModeration.revision,
      note: schema.chartModeration.note,
    })
    .from(schema.chartModeration)
    .where(inArray(schema.chartModeration.chartSha256, shaList))
  const currentBySha = new Map(currentRows.map((row) => [row.sha256, row]))

  const now = new Date()
  const batchId = input.items.length > 1 ? randomUUID() : undefined
  const table = schema.chartModeration
  const statements: BatchItem<'sqlite'>[] = []
  const results = new Map<string, ChartModerationChangeResult>()
  const pending = new Map<string, string>()

  for (const item of input.items) {
    const current = currentBySha.get(item.sha256)
    const currentStatus = current?.status ?? 'active'
    const currentRevision = current?.revision ?? 0
    if (currentRevision !== item.expectedRevision) {
      results.set(item.sha256, {
        sha256: item.sha256,
        result: 'conflict',
        status: currentStatus,
        revision: currentRevision,
        audit_log_id: null,
      })
      continue
    }
    if (currentStatus === input.status) {
      results.set(item.sha256, {
        sha256: item.sha256,
        result: 'unchanged',
        status: currentStatus,
        revision: currentRevision,
        audit_log_id: null,
      })
      continue
    }

    const changeId = randomUUID()
    pending.set(item.sha256, changeId)
    const values = {
      chartSha256: item.sha256,
      status: input.status,
      reason: input.reason,
      note: input.note ?? current?.note ?? '',
      lastChangeId: changeId,
      updatedBy: input.actorId,
      updatedAt: now,
    }
    statements.push(
      currentRevision === 0
        ? db
            .insert(table)
            .values({ ...values, revision: 1 })
            .onConflictDoNothing()
        : db
            .update(table)
            .set({ ...values, revision: sql`${table.revision} + 1` })
            .where(and(eq(table.chartSha256, item.sha256), eq(table.revision, currentRevision))),
      auditInsertStatement({
        id: changeId,
        actorUserId: input.actorId,
        targetType: 'chart',
        targetId: item.sha256,
        action: CHART_ACTIONS[input.status],
        previousStatus: currentStatus,
        newStatus: input.status,
        reason: input.reason,
        metadata: {
          revision: currentRevision + 1,
          ...(input.note !== undefined ? { note: input.note } : {}),
          ...(batchId ? { batch_id: batchId, batch_size: input.items.length } : {}),
        },
        createdAt: now,
        condition: sql`exists (select 1 from ${table} where ${table.chartSha256} = ${item.sha256} and ${table.lastChangeId} = ${changeId})`,
      }),
    )
  }

  if (statements.length > 0) {
    await db.batch(statements as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]])
    const afterRows = await db
      .select({
        sha256: table.chartSha256,
        status: table.status,
        revision: table.revision,
        lastChangeId: table.lastChangeId,
      })
      .from(table)
      .where(inArray(table.chartSha256, [...pending.keys()]))
    const afterBySha = new Map(afterRows.map((row) => [row.sha256, row]))
    for (const [sha256, changeId] of pending) {
      const after = afterBySha.get(sha256)
      const applied = after?.lastChangeId === changeId
      results.set(sha256, {
        sha256,
        result: applied ? 'changed' : 'conflict',
        status: after?.status ?? 'active',
        revision: after?.revision ?? 0,
        audit_log_id: applied ? changeId : null,
      })
    }
  }

  return input.items.map((item) => results.get(item.sha256)!)
}

function auditInsertStatement(entry: {
  id: string
  actorUserId: string
  targetType: 'user' | 'chart'
  targetId: string
  action: ModerationAction
  previousStatus: string
  newStatus: string
  reason: string
  metadata: Record<string, unknown>
  createdAt: Date
  condition: SQL
}) {
  const logs = schema.moderationAuditLogs
  // INSERT ... SELECT ... WHERE で、状態変更が実際に適用された場合だけ記録する。
  // D1 の batch はクエリビルダーしか受け付けない（`db.run(sql)` は bind できず失敗する）ため、
  // insert().select() で組み立てる。SELECT の列順は moderation_audit_logs の列定義順。
  return db.insert(logs).select(
    sql`select ${entry.id}, ${entry.actorUserId}, ${entry.targetType}, ${entry.targetId},
      ${entry.action}, ${entry.previousStatus}, ${entry.newStatus}, ${entry.reason},
      ${JSON.stringify(entry.metadata)}, ${entry.createdAt.getTime()}
    where ${entry.condition}`,
  )
}

// ---------------------------------------------------------------------------
// 検索・詳細
// ---------------------------------------------------------------------------

export async function searchUsers(input: {
  search: string
  status?: UserModerationStatus
  limit: number
  offset: number
}) {
  const conditions: SQL[] = []
  if (input.search) {
    const search = input.search
    conditions.push(
      or(
        eq(schema.users.id, search),
        likeEscaped(schema.profiles.displayName, search),
        likeEscaped(schema.users.email, search.toLowerCase()),
      )!,
    )
  }
  if (input.status === 'active') {
    conditions.push(
      or(isNull(schema.userModeration.status), eq(schema.userModeration.status, 'active'))!,
    )
  } else if (input.status) {
    conditions.push(eq(schema.userModeration.status, input.status))
  }
  const where = conditions.length > 0 ? and(...conditions) : undefined
  const base = () =>
    db
      .select({
        id: schema.users.id,
        email: schema.users.email,
        display_name: schema.profiles.displayName,
        status: schema.userModeration.status,
        revision: schema.userModeration.revision,
        moderation_updated_at: schema.userModeration.updatedAt,
        created_at: schema.users.createdAt,
      })
      .from(schema.users)
      .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
      .leftJoin(schema.userModeration, eq(schema.userModeration.userId, schema.users.id))
  const [rows, totals] = await Promise.all([
    base()
      .where(where)
      .orderBy(desc(schema.users.createdAt), schema.users.id)
      .limit(input.limit)
      .offset(input.offset),
    db
      .select({ total: count() })
      .from(schema.users)
      .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
      .leftJoin(schema.userModeration, eq(schema.userModeration.userId, schema.users.id))
      .where(where),
  ])
  const total = Number(totals[0]?.total ?? 0)
  return {
    users: rows.map((row) => ({
      id: row.id,
      email: row.email,
      display_name: row.display_name ?? '',
      status: row.status ?? 'active',
      revision: row.revision ?? 0,
      moderation_updated_at: row.moderation_updated_at?.toISOString() ?? null,
      created_at: row.created_at.toISOString(),
    })),
    pagination: pagination(input.limit, input.offset, total),
  }
}

export async function getUserModerationDetail(userId: string, adminUserIds: Set<string>) {
  const rows = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      created_at: schema.users.createdAt,
      display_name: schema.profiles.displayName,
      bio: schema.profiles.bio,
      status: schema.userModeration.status,
      reason: schema.userModeration.reason,
      revision: schema.userModeration.revision,
      updated_by: schema.userModeration.updatedBy,
      updated_at: schema.userModeration.updatedAt,
    })
    .from(schema.users)
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .leftJoin(schema.userModeration, eq(schema.userModeration.userId, schema.users.id))
    .where(eq(schema.users.id, userId))
    .limit(1)
  const user = rows[0]
  if (!user) {
    return null
  }

  const [scoreCounts, bestCounts, courseCounts, latestScores, activeSessions] = await Promise.all([
    db
      .select({
        total: count(),
        accepted: sql<number>`coalesce(sum(case when ${schema.scores.accepted} then 1 else 0 end), 0)`,
        last_received_at: sql<number | null>`max(${schema.scores.serverReceivedAt})`,
      })
      .from(schema.scores)
      .where(eq(schema.scores.playerId, userId)),
    db
      .select({ total: count() })
      .from(schema.bestScores)
      .where(eq(schema.bestScores.playerId, userId)),
    db
      .select({ total: count() })
      .from(schema.courseScores)
      .where(eq(schema.courseScores.playerId, userId)),
    db
      .select({
        score_id: schema.scores.id,
        chart_sha256: schema.scores.chartSha256,
        title: schema.charts.title,
        artist: schema.charts.artist,
        clear: schema.scores.clearType,
        ex_score: schema.scores.exScore,
        accepted: schema.scores.accepted,
        server_received_at: schema.scores.serverReceivedAt,
        chart_status: schema.chartModeration.status,
      })
      .from(schema.scores)
      .leftJoin(schema.charts, eq(schema.charts.sha256, schema.scores.chartSha256))
      .leftJoin(
        schema.chartModeration,
        eq(schema.chartModeration.chartSha256, schema.scores.chartSha256),
      )
      .where(eq(schema.scores.playerId, userId))
      .orderBy(desc(schema.scores.serverReceivedAt), desc(schema.scores.id))
      .limit(20),
    db
      .select({ total: count() })
      .from(schema.sessions)
      .where(
        and(
          eq(schema.sessions.userId, userId),
          eq(schema.sessions.kind, 'refresh'),
          isNull(schema.sessions.revokedAt),
          sql`${schema.sessions.expiresAt} > ${Date.now()}`,
        ),
      ),
  ])
  const actorNames = await displayNames(user.updated_by ? [user.updated_by] : [])
  const lastReceivedAt = scoreCounts[0]?.last_received_at

  return {
    user: {
      id: user.id,
      email: user.email,
      display_name: user.display_name ?? '',
      bio: user.bio ?? '',
      created_at: user.created_at.toISOString(),
      is_admin: adminUserIds.has(user.id),
    },
    moderation: {
      status: user.status ?? 'active',
      reason: user.reason ?? '',
      revision: user.revision ?? 0,
      updated_at: user.updated_at?.toISOString() ?? null,
      updated_by: user.updated_by
        ? { id: user.updated_by, display_name: actorNames.get(user.updated_by) ?? '' }
        : null,
    },
    stats: {
      score_count: Number(scoreCounts[0]?.total ?? 0),
      accepted_score_count: Number(scoreCounts[0]?.accepted ?? 0),
      best_score_count: Number(bestCounts[0]?.total ?? 0),
      course_score_count: Number(courseCounts[0]?.total ?? 0),
      active_session_count: Number(activeSessions[0]?.total ?? 0),
      last_score_at: lastReceivedAt == null ? null : new Date(lastReceivedAt).toISOString(),
    },
    latest_scores: latestScores.map((row) => ({
      ...row,
      title: row.title ?? '',
      chart_status: row.chart_status ?? 'active',
      server_received_at: row.server_received_at.toISOString(),
    })),
  }
}

export async function searchCharts(input: {
  search: string
  field: 'any' | 'sha256' | 'md5' | 'title' | 'artist'
  status?: ChartModerationStatus
  limit: number
  offset: number
}) {
  // active 以外の状態で絞るときは moderation 行を起点にし、未登録の事前ブロックも含める。
  const fromModeration = input.status !== undefined && input.status !== 'active'
  const conditions: SQL[] = []
  const search = input.search
  if (search) {
    const sha = fromModeration ? schema.chartModeration.chartSha256 : schema.charts.sha256
    switch (input.field) {
      case 'sha256':
        conditions.push(eq(sha, search.toLowerCase()))
        break
      case 'md5':
        conditions.push(eq(schema.charts.md5, search.toLowerCase()))
        break
      case 'title':
        conditions.push(likeEscaped(schema.charts.title, search))
        break
      case 'artist':
        conditions.push(likeEscaped(schema.charts.artist, search))
        break
      default:
        conditions.push(
          or(
            eq(sha, search.toLowerCase()),
            eq(schema.charts.md5, search.toLowerCase()),
            likeEscaped(schema.charts.title, search),
            likeEscaped(schema.charts.artist, search),
          )!,
        )
    }
  }
  if (input.status === 'active') {
    conditions.push(
      or(isNull(schema.chartModeration.status), eq(schema.chartModeration.status, 'active'))!,
    )
  } else if (input.status) {
    conditions.push(eq(schema.chartModeration.status, input.status))
  }
  const where = conditions.length > 0 ? and(...conditions) : undefined
  const fields = {
    sha256: sql<string>`coalesce(${schema.charts.sha256}, ${schema.chartModeration.chartSha256})`,
    registered: sql<number>`${schema.charts.sha256} is not null`,
    md5: schema.charts.md5,
    title: schema.charts.title,
    subtitle: schema.charts.subtitle,
    artist: schema.charts.artist,
    mode: schema.charts.mode,
    level: schema.charts.level,
    status: schema.chartModeration.status,
    reason: schema.chartModeration.reason,
    revision: schema.chartModeration.revision,
    moderation_updated_at: schema.chartModeration.updatedAt,
  }
  const listQuery = fromModeration
    ? db
        .select(fields)
        .from(schema.chartModeration)
        .leftJoin(schema.charts, eq(schema.charts.sha256, schema.chartModeration.chartSha256))
        .where(where)
        .orderBy(desc(schema.chartModeration.updatedAt), schema.chartModeration.chartSha256)
    : db
        .select(fields)
        .from(schema.charts)
        .leftJoin(
          schema.chartModeration,
          eq(schema.chartModeration.chartSha256, schema.charts.sha256),
        )
        .where(where)
        .orderBy(desc(schema.charts.updatedAt), schema.charts.sha256)
  const countQuery = fromModeration
    ? db
        .select({ total: count() })
        .from(schema.chartModeration)
        .leftJoin(schema.charts, eq(schema.charts.sha256, schema.chartModeration.chartSha256))
        .where(where)
    : db
        .select({ total: count() })
        .from(schema.charts)
        .leftJoin(
          schema.chartModeration,
          eq(schema.chartModeration.chartSha256, schema.charts.sha256),
        )
        .where(where)
  let [rows, totals] = await Promise.all([
    listQuery.limit(input.limit).offset(input.offset),
    countQuery,
  ])

  // 未登録のSHA-256を完全一致で検索したときは、事前ブロック用に moderation 行または空行を返す。
  if (
    rows.length === 0 &&
    input.offset === 0 &&
    !fromModeration &&
    /^[0-9a-f]{64}$/i.test(search) &&
    (input.field === 'sha256' || input.field === 'any')
  ) {
    const sha256 = search.toLowerCase()
    const moderation = await db
      .select(fields)
      .from(schema.chartModeration)
      .leftJoin(schema.charts, eq(schema.charts.sha256, schema.chartModeration.chartSha256))
      .where(eq(schema.chartModeration.chartSha256, sha256))
      .limit(1)
    const matchesStatus = (status: string | null) =>
      !input.status || (status ?? 'active') === input.status
    if (moderation[0] && matchesStatus(moderation[0].status)) {
      rows = moderation
    } else if (!moderation[0] && matchesStatus(null)) {
      rows = [
        {
          sha256,
          registered: 0,
          md5: null,
          title: '',
          subtitle: null,
          artist: null,
          mode: '',
          level: null,
          status: null,
          reason: null,
          revision: null,
          moderation_updated_at: null,
        },
      ]
    }
    totals = [{ total: rows.length }]
  }

  const stats = await chartScoreStats(rows.map((row) => row.sha256))
  const total = Number(totals[0]?.total ?? 0)
  return {
    charts: rows.map((row) => ({
      sha256: row.sha256,
      registered: Boolean(row.registered),
      md5: row.md5,
      title: row.title ?? '',
      subtitle: row.subtitle,
      artist: row.artist,
      mode: row.mode,
      level: row.level,
      status: row.status ?? 'active',
      reason: row.reason ?? '',
      revision: row.revision ?? 0,
      moderation_updated_at: row.moderation_updated_at?.toISOString() ?? null,
      player_count: stats.get(row.sha256)?.player_count ?? 0,
      score_count: stats.get(row.sha256)?.score_count ?? 0,
    })),
    pagination: pagination(input.limit, input.offset, total),
  }
}

/**
 * ユーザーがスコアを送った譜面の一覧。1人のユーザーが大量に登録した譜面を、ユーザー詳細から
 * まとめて制限するために使う。`soleOnly` ではそのユーザーしかスコアを送っていない譜面に絞る。
 * 各行の revision は譜面一覧と同じく、一括操作の楽観ロックに使う。
 */
export async function listUserCharts(input: {
  userId: string
  soleOnly: boolean
  limit: number
  offset: number
}) {
  const scores = schema.scores
  const others = alias(scores, 'other_player_score')
  const where = and(
    eq(scores.playerId, input.userId),
    input.soleOnly
      ? notExists(
          db
            .select({ id: others.id })
            .from(others)
            .where(
              and(eq(others.chartSha256, scores.chartSha256), ne(others.playerId, input.userId)),
            ),
        )
      : undefined,
  )
  const lastPlayedAt = sql<number>`max(${scores.serverReceivedAt})`
  const [played, totals] = await Promise.all([
    db
      .select({
        sha256: scores.chartSha256,
        user_score_count: count(),
        last_played_at: lastPlayedAt,
      })
      .from(scores)
      .where(where)
      .groupBy(scores.chartSha256)
      .orderBy(desc(lastPlayedAt), scores.chartSha256)
      .limit(input.limit)
      .offset(input.offset),
    db
      .select({ total: sql<number>`count(distinct ${scores.chartSha256})` })
      .from(scores)
      .where(where),
  ])
  const shaList = played.map((row) => row.sha256)
  const [chartRows, stats] = await Promise.all([
    inChunks(shaList, (chunk) =>
      db
        .select({
          sha256: schema.charts.sha256,
          md5: schema.charts.md5,
          title: schema.charts.title,
          subtitle: schema.charts.subtitle,
          artist: schema.charts.artist,
          mode: schema.charts.mode,
          level: schema.charts.level,
          status: schema.chartModeration.status,
          reason: schema.chartModeration.reason,
          revision: schema.chartModeration.revision,
          moderation_updated_at: schema.chartModeration.updatedAt,
        })
        .from(schema.charts)
        .leftJoin(
          schema.chartModeration,
          eq(schema.chartModeration.chartSha256, schema.charts.sha256),
        )
        .where(inArray(schema.charts.sha256, chunk)),
    ),
    chartScoreStats(shaList),
  ])
  const charts = new Map(chartRows.map((row) => [row.sha256, row]))
  return {
    charts: played.map((row) => {
      const chart = charts.get(row.sha256)
      return {
        sha256: row.sha256,
        registered: Boolean(chart),
        md5: chart?.md5 ?? null,
        title: chart?.title ?? '',
        subtitle: chart?.subtitle ?? null,
        artist: chart?.artist ?? null,
        mode: chart?.mode ?? null,
        level: chart?.level ?? null,
        status: chart?.status ?? 'active',
        reason: chart?.reason ?? '',
        revision: chart?.revision ?? 0,
        moderation_updated_at: chart?.moderation_updated_at?.toISOString() ?? null,
        player_count: stats.get(row.sha256)?.player_count ?? 0,
        score_count: stats.get(row.sha256)?.score_count ?? 0,
        user_score_count: Number(row.user_score_count),
        last_played_at: new Date(Number(row.last_played_at)).toISOString(),
      }
    }),
    pagination: pagination(input.limit, input.offset, Number(totals[0]?.total ?? 0)),
  }
}

async function chartScoreStats(shaList: string[]) {
  const stats = new Map<string, { player_count: number; score_count: number }>()
  const rows = await inChunks(shaList, (chunk) =>
    db
      .select({
        sha256: schema.scores.chartSha256,
        player_count: sql<number>`count(distinct ${schema.scores.playerId})`,
        score_count: count(),
      })
      .from(schema.scores)
      .where(inArray(schema.scores.chartSha256, chunk))
      .groupBy(schema.scores.chartSha256),
  )
  for (const row of rows) {
    stats.set(row.sha256, {
      player_count: Number(row.player_count),
      score_count: Number(row.score_count),
    })
  }
  return stats
}

export async function getChartModerationDetail(sha256: string) {
  const [charts, moderationRows, stats, players] = await Promise.all([
    db.select().from(schema.charts).where(eq(schema.charts.sha256, sha256)).limit(1),
    db
      .select()
      .from(schema.chartModeration)
      .where(eq(schema.chartModeration.chartSha256, sha256))
      .limit(1),
    chartScoreStats([sha256]),
    db
      .select({
        player_id: schema.scores.playerId,
        display_name: schema.profiles.displayName,
        user_status: schema.userModeration.status,
        score_count: count(),
        best_ex_score: sql<number>`max(${schema.scores.exScore})`,
        last_received_at: sql<number>`max(${schema.scores.serverReceivedAt})`,
      })
      .from(schema.scores)
      .leftJoin(schema.profiles, eq(schema.profiles.id, schema.scores.playerId))
      .leftJoin(schema.userModeration, eq(schema.userModeration.userId, schema.scores.playerId))
      .where(eq(schema.scores.chartSha256, sha256))
      .groupBy(schema.scores.playerId)
      .orderBy(desc(sql`max(${schema.scores.serverReceivedAt})`))
      .limit(100),
  ])
  const chart = charts[0] ?? null
  const moderation = moderationRows[0] ?? null
  if (!chart && !moderation) {
    // 未登録・未管理の譜面も事前ブロックできるよう、空の詳細を返す。
    return {
      sha256,
      chart: null,
      moderation: emptyChartModeration(),
      stats: { player_count: 0, score_count: 0 },
      players: [],
      same_md5_charts: [],
    }
  }
  const sameMd5 = chart?.md5
    ? await db
        .select({
          sha256: schema.charts.sha256,
          title: schema.charts.title,
          status: schema.chartModeration.status,
        })
        .from(schema.charts)
        .leftJoin(
          schema.chartModeration,
          eq(schema.chartModeration.chartSha256, schema.charts.sha256),
        )
        .where(and(eq(schema.charts.md5, chart.md5), sql`${schema.charts.sha256} <> ${sha256}`))
        .limit(20)
    : []
  const actorNames = await displayNames(moderation ? [moderation.updatedBy] : [])
  return {
    sha256,
    chart: chart
      ? {
          sha256: chart.sha256,
          md5: chart.md5,
          title: chart.title,
          subtitle: chart.subtitle,
          artist: chart.artist,
          genre: chart.genre,
          mode: chart.mode,
          level: chart.level,
          difficulty: chart.difficulty,
          notes: chart.notes,
          created_at: chart.createdAt.toISOString(),
        }
      : null,
    moderation: moderation
      ? {
          status: moderation.status,
          reason: moderation.reason,
          note: moderation.note,
          revision: moderation.revision,
          updated_at: moderation.updatedAt.toISOString(),
          updated_by: {
            id: moderation.updatedBy,
            display_name: actorNames.get(moderation.updatedBy) ?? '',
          },
        }
      : emptyChartModeration(),
    stats: stats.get(sha256) ?? { player_count: 0, score_count: 0 },
    players: players.map((row) => ({
      player_id: row.player_id,
      display_name: row.display_name ?? '',
      user_status: row.user_status ?? 'active',
      score_count: Number(row.score_count),
      best_ex_score: Number(row.best_ex_score),
      last_received_at: new Date(Number(row.last_received_at)).toISOString(),
    })),
    // 参考表示のみ。MD5 一致だけを根拠に別 SHA-256 をまとめて変更しない。
    same_md5_charts: sameMd5.map((row) => ({ ...row, status: row.status ?? 'active' })),
  }
}

function emptyChartModeration() {
  return {
    status: 'active' as const,
    reason: '',
    note: '',
    revision: 0,
    updated_at: null,
    updated_by: null,
  }
}

export async function listAuditLogs(input: {
  targetType?: 'user' | 'chart'
  targetId?: string
  actorId?: string
  limit: number
  offset: number
}) {
  const logs = schema.moderationAuditLogs
  const conditions: SQL[] = []
  if (input.targetType) conditions.push(eq(logs.targetType, input.targetType))
  if (input.targetId) conditions.push(eq(logs.targetId, input.targetId))
  if (input.actorId) conditions.push(eq(logs.actorUserId, input.actorId))
  const where = conditions.length > 0 ? and(...conditions) : undefined
  const [rows, totals] = await Promise.all([
    db
      .select()
      .from(logs)
      .where(where)
      .orderBy(desc(logs.createdAt), desc(logs.id))
      .limit(input.limit)
      .offset(input.offset),
    db.select({ total: count() }).from(logs).where(where),
  ])
  const names = await displayNames([
    ...new Set(
      rows.flatMap((row) => [
        row.actorUserId,
        ...(row.targetType === 'user' ? [row.targetId] : []),
      ]),
    ),
  ])
  const chartShas = [
    ...new Set(rows.filter((row) => row.targetType === 'chart').map((row) => row.targetId)),
  ]
  const chartTitles = new Map(
    (
      await inChunks(chartShas, (chunk) =>
        db
          .select({ sha256: schema.charts.sha256, title: schema.charts.title })
          .from(schema.charts)
          .where(inArray(schema.charts.sha256, chunk)),
      )
    ).map((row) => [row.sha256, row.title]),
  )
  const total = Number(totals[0]?.total ?? 0)
  return {
    logs: rows.map((row) => ({
      id: row.id,
      created_at: row.createdAt.toISOString(),
      actor: { id: row.actorUserId, display_name: names.get(row.actorUserId) ?? '' },
      action: row.action,
      target_type: row.targetType,
      target_id: row.targetId,
      target_label:
        row.targetType === 'user'
          ? (names.get(row.targetId) ?? '')
          : (chartTitles.get(row.targetId) ?? ''),
      previous_status: row.previousStatus,
      new_status: row.newStatus,
      reason: row.reason,
      metadata: row.metadata,
    })),
    pagination: pagination(input.limit, input.offset, total),
  }
}

async function displayNames(ids: string[]): Promise<Map<string, string>> {
  const rows = await inChunks(ids, (chunk) =>
    db
      .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
      .from(schema.profiles)
      .where(inArray(schema.profiles.id, chunk)),
  )
  return new Map(rows.map((row) => [row.id, row.display_name]))
}

// D1 は1クエリあたり bind parameter が100個まで。IN 句は分割して実行する。
async function inChunks<T>(values: string[], run: (chunk: string[]) => Promise<T[]>) {
  const rows: T[] = []
  for (let offset = 0; offset < values.length; offset += 90) {
    rows.push(...(await run(values.slice(offset, offset + 90))))
  }
  return rows
}

function pagination(limit: number, offset: number, total: number) {
  return { limit, offset, total, has_more: offset + limit < total }
}
