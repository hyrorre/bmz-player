import { createError } from 'h3'
import { requireHex } from '../services/ir'

export interface OwnScoreHistoryCursor {
  server_received_at_ms: number
  score_id: string
}

export function optionalHexQuery(value: unknown, length: number, label: string): string | null {
  if (value == null || value === '') {
    return null
  }
  if (typeof value !== 'string') {
    throw badQuery(`${label} is invalid`)
  }
  try {
    requireHex(value, length, label)
  } catch (error) {
    throw badQuery(error instanceof Error ? error.message : `${label} is invalid`)
  }
  return value
}

export function ownScoreHistoryCursorFromQuery(
  receivedAtValue: unknown,
  scoreIdValue: unknown,
): OwnScoreHistoryCursor | null {
  const hasReceivedAt = receivedAtValue != null
  const hasScoreId = scoreIdValue != null
  if (!hasReceivedAt && !hasScoreId) {
    return null
  }
  if (!hasReceivedAt || !hasScoreId || typeof scoreIdValue !== 'string' || !scoreIdValue) {
    throw badQuery('score cursor is invalid')
  }
  const serverReceivedAtMs = Number(receivedAtValue)
  if (!Number.isSafeInteger(serverReceivedAtMs) || serverReceivedAtMs < 0) {
    throw badQuery('score cursor is invalid')
  }
  return { server_received_at_ms: serverReceivedAtMs, score_id: scoreIdValue }
}

export function ownScoreHistoryPage<T extends { score_id: string; server_received_at: Date }>(
  rows: T[],
  limit: number,
): {
  rows: T[]
  hasMore: boolean
  nextCursor?: OwnScoreHistoryCursor
} {
  const hasMore = rows.length > limit
  const visibleRows = rows.slice(0, limit)
  const cursorRow = hasMore ? visibleRows.at(-1) : undefined
  return {
    rows: visibleRows,
    hasMore,
    nextCursor: cursorRow
      ? {
          server_received_at_ms: cursorRow.server_received_at.getTime(),
          score_id: cursorRow.score_id,
        }
      : undefined,
  }
}

function badQuery(statusMessage: string) {
  return createError({ statusCode: 400, statusMessage })
}
