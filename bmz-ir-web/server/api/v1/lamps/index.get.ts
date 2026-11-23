import { createError, defineEventHandler, getQuery } from 'h3'
import { loadDifficultyDistribution } from '../../../services/lamp_distribution'
import { distributionMode, lampPlayerId, lampQueryText } from '../../../utils/lamp_query'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const table = lampQueryText(query.table, 'table')
  const mode = distributionMode(query.mode)
  const report = await loadDifficultyDistribution(
    await lampPlayerId(event, query.player),
    mode,
    table,
    query.include === 'first_pages',
  )
  if (!report) throw createError({ statusCode: 404, statusMessage: 'Player not found' })
  return report
})
