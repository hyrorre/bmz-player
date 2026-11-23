import { createError, defineEventHandler, getQuery } from 'h3'
import { IR_CLEAR_LABELS, IR_SCORE_RANK_LABELS } from '../../../../shared/constants/ir'
import { clampInteger } from '../../../services/ir/common'
import { loadDistributionCharts } from '../../../services/lamp_distribution'
import { distributionMode, lampPlayerId, lampQueryText } from '../../../utils/lamp_query'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const table = lampQueryText(query.table, 'table')
  const mode = distributionMode(query.mode)
  const { level } = query
  const labels = mode === 'score' ? IR_SCORE_RANK_LABELS : IR_CLEAR_LABELS
  const rank =
    typeof query.rank === 'string' && /^[0-9]$/.test(query.rank) ? Number(query.rank) : -1
  if (!table || typeof level !== 'string' || rank < 0 || rank >= labels.length) {
    throw createError({
      statusCode: 400,
      statusMessage: 'table, level, and a valid rank are required',
    })
  }
  const result = await loadDistributionCharts(await lampPlayerId(event, query.player), {
    mode,
    table,
    level,
    rank,
    limit: clampInteger(query.limit, 50, 1, 100),
    offset: clampInteger(query.offset, 0, 0, 100_000),
  })
  if (!result) throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  return result
})
