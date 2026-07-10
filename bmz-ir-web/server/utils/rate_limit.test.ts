import { describe, expect, test } from 'bun:test'
import { RATE_LIMIT_WINDOW_MS, __test } from './rate_limit'

describe('rate limit helpers', () => {
  test('increments attempts through one atomic upsert returning the stored count', async () => {
    let insertCalls = 0
    let insertedValues: Record<string, unknown> | undefined
    let conflictConfig: { target?: unknown[]; set?: Record<string, unknown> } | undefined
    const database = {
      insert() {
        insertCalls += 1
        return {
          values(values: Record<string, unknown>) {
            insertedValues = values
            return {
              onConflictDoUpdate(config: { target?: unknown[]; set?: Record<string, unknown> }) {
                conflictConfig = config
                return {
                  returning() {
                    return {
                      async get() {
                        return { attempts: 7 }
                      },
                    }
                  },
                }
              },
            }
          },
        }
      },
    } as unknown as Parameters<typeof __test.incrementRateLimitAttempt>[0]

    const attempts = await __test.incrementRateLimitAttempt(database, {
      action: 'score_submit',
      scope: 'user',
      scopeHash: 'hash',
      windowStart: new Date(0),
      now: new Date(1),
    })

    expect(attempts).toBe(7)
    expect(insertCalls).toBe(1)
    expect(insertedValues?.attempts).toBe(1)
    expect(conflictConfig?.target).toHaveLength(4)
    expect(conflictConfig?.set?.attempts).toBeDefined()
  })

  test('floors timestamps to the active rate limit window', () => {
    const nowMs = Date.UTC(2026, 6, 11, 3, 22, 45, 678)

    expect(__test.rateLimitWindowStart(nowMs).getTime()).toBe(
      nowMs - (nowMs % RATE_LIMIT_WINDOW_MS),
    )
  })

  test('keeps Retry-After rounded up and at least one second', () => {
    const windowStart = new Date(Date.UTC(2026, 6, 11, 3, 15, 0, 0))

    expect(
      __test.retryAfterFromWindowStart(windowStart, windowStart.getTime() + 14 * 60_000 + 1),
    ).toBe(60)
    expect(
      __test.retryAfterFromWindowStart(
        windowStart,
        windowStart.getTime() + RATE_LIMIT_WINDOW_MS - 1,
      ),
    ).toBe(1)
  })
})
