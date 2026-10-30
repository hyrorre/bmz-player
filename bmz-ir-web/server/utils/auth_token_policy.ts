export type SessionRevocationReason =
  'logout' | 'rotated' | 'password_changed' | 'reuse_detected' | 'admin'

export function isRecentRotatedRefreshRetry(
  revokedAt: Date,
  revokedReason: SessionRevocationReason | null,
  now: number,
) {
  const benignRetryWindowMs = 5 * 60 * 1000
  return revokedReason === 'rotated' && revokedAt.getTime() >= now - benignRetryWindowMs
}

/**
 * 失効済みrefresh tokenが提示されたときの扱い。
 *
 * - `retry`: rotation直後の並行リトライ。拒否するだけで何も失効させない。
 * - `reuse_detected`: rotation済み・再利用検知済みのtokenの再利用。盗用の疑いとして
 *   そのユーザーの全セッションを失効させる。
 * - `reject`: ログアウト・パスワード変更・管理者操作で失効したtoken。失効は所有者や
 *   管理者の意図どおりで、その後に作られた正常なセッションとは無関係なので拒否だけする。
 *   失効理由が不明な行も、無関係なセッションを巻き込まないよう拒否だけにする。
 */
export function revokedRefreshTokenAction(
  revokedAt: Date,
  revokedReason: SessionRevocationReason | null,
  now: number,
): 'retry' | 'reuse_detected' | 'reject' {
  switch (revokedReason) {
    case 'rotated':
      return isRecentRotatedRefreshRetry(revokedAt, revokedReason, now) ? 'retry' : 'reuse_detected'
    case 'reuse_detected':
      return 'reuse_detected'
    default:
      return 'reject'
  }
}
