export type SessionRevocationReason =
  | 'logout'
  | 'rotated'
  | 'password_changed'
  | 'reuse_detected'
  | 'admin'

export function isRecentRotatedRefreshRetry(
  revokedAt: Date,
  revokedReason: SessionRevocationReason | null,
  now: number,
) {
  const benignRetryWindowMs = 5 * 60 * 1000
  return revokedReason === 'rotated' && revokedAt.getTime() >= now - benignRetryWindowMs
}
