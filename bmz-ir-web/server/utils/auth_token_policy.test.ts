import { describe, expect, test } from 'bun:test'
import { isRecentRotatedRefreshRetry, revokedRefreshTokenAction } from './auth_token_policy'

describe('auth token policy', () => {
  test('treats a recently rotated refresh token as a benign concurrent retry', () => {
    const now = Date.parse('2026-06-18T02:00:00.000Z')

    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), 'rotated', now)).toBe(true)
    expect(isRecentRotatedRefreshRetry(new Date(now - 6 * 60_000), 'rotated', now)).toBe(false)
    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), 'logout', now)).toBe(false)
    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), null, now)).toBe(false)
  })

  test('only rotated or already detected token reuse revokes other sessions', () => {
    const now = Date.parse('2026-06-18T02:00:00.000Z')
    const recent = new Date(now - 30_000)
    const old = new Date(now - 60 * 60_000)

    expect(revokedRefreshTokenAction(recent, 'rotated', now)).toBe('retry')
    expect(revokedRefreshTokenAction(old, 'rotated', now)).toBe('reuse_detected')
    expect(revokedRefreshTokenAction(old, 'reuse_detected', now)).toBe('reuse_detected')
    for (const reason of ['logout', 'password_changed', 'admin', null] as const) {
      expect(revokedRefreshTokenAction(recent, reason, now)).toBe('reject')
      expect(revokedRefreshTokenAction(old, reason, now)).toBe('reject')
    }
  })
})
