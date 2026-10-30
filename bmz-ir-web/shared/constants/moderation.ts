/** ユーザーの利用状態。行が無いユーザーは active。 */
export const USER_MODERATION_STATUSES = ['active', 'submit_suspended', 'banned'] as const
export type UserModerationStatus = (typeof USER_MODERATION_STATUSES)[number]

/** 譜面の公開状態。行が無い譜面は active。 */
export const CHART_MODERATION_STATUSES = ['active', 'under_review', 'hidden', 'blocked'] as const
export type ChartModerationStatus = (typeof CHART_MODERATION_STATUSES)[number]

/** 公開読み取りから除外し、新規スコア送信も拒否する譜面状態。 */
export const NON_PUBLIC_CHART_STATUSES = [
  'hidden',
  'blocked',
] as const satisfies readonly ChartModerationStatus[]

export const MODERATION_ACTIONS = [
  'user_suspend',
  'user_ban',
  'user_restore',
  'chart_review',
  'chart_hide',
  'chart_block',
  'chart_restore',
] as const
export type ModerationAction = (typeof MODERATION_ACTIONS)[number]

/**
 * モデレーションによる拒否の機械判定用コード。HTTP status はすべて 403 で、
 * 応答 body の `data.code` に入る。クライアントは再送しても解除されない拒否として扱う。
 */
export const MODERATION_ERROR_CODES = [
  'ACCOUNT_SUSPENDED',
  'ACCOUNT_BANNED',
  'CHART_HIDDEN',
  'CHART_BLOCKED',
] as const
export type ModerationErrorCode = (typeof MODERATION_ERROR_CODES)[number]

export const MODERATION_ERROR_MESSAGES: Record<ModerationErrorCode, string> = {
  ACCOUNT_SUSPENDED: 'Score submission is suspended for this account',
  ACCOUNT_BANNED: 'This account is banned from BMZ IR',
  CHART_HIDDEN: 'Score submission is temporarily disabled for this chart',
  CHART_BLOCKED: 'Score submission is blocked for this chart',
}

export const MAX_MODERATION_REASON_LENGTH = 1000
export const MAX_MODERATION_NOTE_LENGTH = 2000
/** 一括状態変更で1リクエストに含められる譜面数。 */
export const MAX_CHART_MODERATION_BATCH_SIZE = 50
