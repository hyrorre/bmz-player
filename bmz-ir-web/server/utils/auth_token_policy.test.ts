import { describe, expect, test } from 'bun:test'
import { isRecentRotatedRefreshRetry } from './auth_token_policy'

describe('auth token policy', () => {
  test('treats a recently rotated refresh token as a benign concurrent retry', () => {
    const now = Date.parse('2026-06-18T02:00:00.000Z')

    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), 'rotated', now)).toBe(true)
    expect(isRecentRotatedRefreshRetry(new Date(now - 6 * 60_000), 'rotated', now)).toBe(false)
    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), 'logout', now)).toBe(false)
    expect(isRecentRotatedRefreshRetry(new Date(now - 30_000), null, now)).toBe(false)
  })
})
