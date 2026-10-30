import { eq, inArray } from 'drizzle-orm'
import { createError } from 'h3'
import { db, schema } from 'hub:db'
import {
  MODERATION_ERROR_MESSAGES,
  type ChartModerationStatus,
  type ModerationErrorCode,
  type UserModerationStatus,
} from '../../../shared/constants/moderation'

/** モデレーションによる拒否。HTTP 応答では 403 と `data.code` に変換する。 */
export class IrModerationError extends Error {
  constructor(readonly code: ModerationErrorCode) {
    super(MODERATION_ERROR_MESSAGES[code])
  }
}

export function moderationHttpError(code: ModerationErrorCode) {
  // 管理者向けの理由は含めず、状態を識別するコードだけを返す。
  return createError({
    statusCode: 403,
    statusMessage: MODERATION_ERROR_MESSAGES[code],
    data: { code },
  })
}

/** API handler 用。IrModerationError なら 403 に変換し、それ以外はそのまま返す。 */
export function toModerationHttpError(error: unknown): unknown {
  return error instanceof IrModerationError ? moderationHttpError(error.code) : error
}

export async function getUserModerationStatus(userId: string): Promise<UserModerationStatus> {
  const rows = await db
    .select({ status: schema.userModeration.status })
    .from(schema.userModeration)
    .where(eq(schema.userModeration.userId, userId))
    .limit(1)
  return rows[0]?.status ?? 'active'
}

export async function getChartModerationStatuses(
  sha256List: string[],
): Promise<Map<string, ChartModerationStatus>> {
  const unique = [...new Set(sha256List)]
  const statuses = new Map<string, ChartModerationStatus>()
  // D1 の bind 上限 (100) を超えないよう分割する。
  for (let offset = 0; offset < unique.length; offset += 90) {
    const chunk = unique.slice(offset, offset + 90)
    const rows = await db
      .select({ sha256: schema.chartModeration.chartSha256, status: schema.chartModeration.status })
      .from(schema.chartModeration)
      .where(inArray(schema.chartModeration.chartSha256, chunk))
    for (const row of rows) {
      statuses.set(row.sha256, row.status)
    }
  }
  return statuses
}

export function userStatusError(status: UserModerationStatus): ModerationErrorCode | null {
  switch (status) {
    case 'banned':
      return 'ACCOUNT_BANNED'
    case 'submit_suspended':
      return 'ACCOUNT_SUSPENDED'
    default:
      return null
  }
}

export function chartStatusError(
  status: ChartModerationStatus | undefined,
): ModerationErrorCode | null {
  switch (status) {
    case 'blocked':
      return 'CHART_BLOCKED'
    case 'hidden':
      return 'CHART_HIDDEN'
    default:
      return null
  }
}

/**
 * スコア・コーススコア・リプレイ・attestation の書き込み前に呼ぶ。
 *
 * ユーザー状態 → 譜面状態の順に、サーバーに保存された sha256 と管理状態だけで判定する。
 * idempotency key による既存スコアの確認より前に呼び、再送でも同じ拒否を返す。
 */
export async function assertCanSubmit(userId: string, chartSha256List: string[]): Promise<void> {
  const userError = userStatusError(await getUserModerationStatus(userId))
  if (userError) {
    throw new IrModerationError(userError)
  }
  if (chartSha256List.length === 0) {
    return
  }
  const statuses = await getChartModerationStatuses(chartSha256List)
  // blocked を hidden より優先して返す。
  let hidden = false
  for (const status of statuses.values()) {
    const error = chartStatusError(status)
    if (error === 'CHART_BLOCKED') {
      throw new IrModerationError(error)
    }
    hidden ||= error === 'CHART_HIDDEN'
  }
  if (hidden) {
    throw new IrModerationError('CHART_HIDDEN')
  }
}

/** 既存スコアへの書き込み (リプレイ・attestation) 用。スコアの譜面を DB から引いて判定する。 */
export async function assertCanWriteScore(userId: string, scoreId: string): Promise<void> {
  const rows = await db
    .select({ chartSha256: schema.scores.chartSha256 })
    .from(schema.scores)
    .where(eq(schema.scores.id, scoreId))
    .limit(1)
  await assertCanSubmit(userId, rows[0] ? [rows[0].chartSha256] : [])
}

/** API handler 用。拒否時は IrModerationError ではなく 403 の HTTP エラーを投げる。 */
export async function requireScoreWriteAllowed(userId: string, scoreId: string): Promise<void> {
  try {
    await assertCanWriteScore(userId, scoreId)
  } catch (error) {
    throw toModerationHttpError(error)
  }
}
