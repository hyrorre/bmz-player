import { createError, defineEventHandler, getQuery } from 'h3'
import { loadDifficultyDistribution } from '../../../services/analysis_distribution'
import {
  analysisPlayerId,
  analysisQueryText,
  distributionMode,
} from '../../../utils/analysis_query'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const table = analysisQueryText(query.table, 'table')
  const mode = distributionMode(query.mode)
  const report = await loadDifficultyDistribution(
    await analysisPlayerId(event, query.player),
    mode,
    table,
    query.include === 'first_pages',
  )
  if (!report) throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  return report
})
