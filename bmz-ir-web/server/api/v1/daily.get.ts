import { getQuery } from 'h3'
import { dailyDateRange, loadDailyReport } from '../../services/daily_report'
import { requireIrUser } from '../../utils/auth'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const date = singleQueryValue(query.date, 'date')
  if (!date) {
    throw createError({ statusCode: 400, statusMessage: 'date is required' })
  }
  try {
    dailyDateRange(date, 0)
  } catch (error) {
    throw createError({
      statusCode: 400,
      statusMessage: error instanceof Error ? error.message : 'date is invalid',
    })
  }

  const mode = singleQueryValue(query.mode, 'mode') ?? 'all'
  if (mode !== 'all') {
    throw createError({ statusCode: 400, statusMessage: 'mode is unsupported' })
  }

  const requestedPlayer = singleQueryValue(query.player, 'player')
  const playerId = requestedPlayer ?? (await requireIrUser(event)).id
  const report = await loadDailyReport({
    playerId,
    date,
    mode,
  })
  if (!report) {
    throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  }
  return report
})

function singleQueryValue(value: unknown, name: string): string | null {
  if (value === undefined) {
    return null
  }
  if (typeof value !== 'string') {
    throw createError({ statusCode: 400, statusMessage: `${name} must be a single value` })
  }
  const normalized = value.trim()
  if (!normalized) {
    throw createError({ statusCode: 400, statusMessage: `${name} must not be empty` })
  }
  return normalized
}
