import { createError, defineEventHandler, getQuery } from 'h3'
import {
  IR_CLEAR_LABELS,
  IR_DISTRIBUTION_PAGE_SIZE,
  IR_SCORE_RANK_LABELS,
} from '../../../../shared/constants/ir'
import { loadDistributionCharts } from '../../../services/analysis_distribution'
import { clampInteger } from '../../../services/ir/common'
import {
  analysisPlayerId,
  analysisQueryText,
  distributionMode,
} from '../../../utils/analysis_query'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const table = analysisQueryText(query.table, 'table')
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
  const result = await loadDistributionCharts(await analysisPlayerId(event, query.player), {
    mode,
    table,
    level,
    rank,
    limit: clampInteger(query.limit, IR_DISTRIBUTION_PAGE_SIZE, 1, 100),
    offset: clampInteger(query.offset, 0, 0, 100_000),
  })
  if (!result) throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  return result
})
