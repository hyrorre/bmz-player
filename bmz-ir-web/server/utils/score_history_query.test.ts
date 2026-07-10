import { describe, expect, test } from 'bun:test'
import {
  optionalHexQuery,
  ownScoreHistoryCursorFromQuery,
  ownScoreHistoryPage,
} from './score_history_query'

describe('own score history query', () => {
  test('returns HTTP 400 for an invalid chart hash', () => {
    expectHttp400(() => optionalHexQuery('invalid', 64, 'chart_sha256'))
    expect(optionalHexQuery('ab'.repeat(32), 64, 'chart_sha256')).toBe('ab'.repeat(32))
  })

  test('requires a complete finite cursor', () => {
    expect(ownScoreHistoryCursorFromQuery(undefined, undefined)).toBeNull()
    expectHttp400(() => ownScoreHistoryCursorFromQuery('1000', undefined))
    expectHttp400(() => ownScoreHistoryCursorFromQuery('-1', 'score-1'))
    expect(ownScoreHistoryCursorFromQuery('1000', 'score-1')).toEqual({
      server_received_at_ms: 1000,
      score_id: 'score-1',
    })
  })

  test('uses the last visible row as the next stable cursor', () => {
    const result = ownScoreHistoryPage(
      [
        { score_id: 'score-3', server_received_at: new Date(3000) },
        { score_id: 'score-2', server_received_at: new Date(2000) },
        { score_id: 'score-1', server_received_at: new Date(1000) },
      ],
      2,
    )

    expect(result.rows.map((row) => row.score_id)).toEqual(['score-3', 'score-2'])
    expect(result.hasMore).toBe(true)
    expect(result.nextCursor).toEqual({ server_received_at_ms: 2000, score_id: 'score-2' })
  })
})

function expectHttp400(callback: () => unknown) {
  try {
    callback()
    throw new Error('expected callback to throw')
  } catch (error) {
    expect(error).toMatchObject({ statusCode: 400 })
  }
}
