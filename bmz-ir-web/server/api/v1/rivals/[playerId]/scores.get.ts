import { getQuery, setResponseHeader } from 'h3'
import { asRuleMode, clampInteger } from '../../../../services/ir/common'
import { loadRivalBestScores } from '../../../../services/rivals'
import { parseRivalScoreCursor } from '../../../../services/rivals_core'
import { requireIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const targetPlayerId = getRouterParam(event, 'playerId')?.trim()
  if (!targetPlayerId || targetPlayerId === user.id) {
    throw createError({ statusCode: 400, statusMessage: 'valid player id is required' })
  }

  const query = getQuery(event)
  const limit = clampInteger(query.limit, 1000, 1, 1000)
  let ruleMode, cursor
  try {
    ruleMode = asRuleMode(query.rule_mode)
    cursor = parseRivalScoreCursor(query.cursor)
  } catch {
    throw createError({ statusCode: 400, statusMessage: 'invalid rule_mode or cursor' })
  }
  const page = await loadRivalBestScores(user.id, targetPlayerId, ruleMode, limit, cursor)
  if (!page) {
    throw createError({ statusCode: 404, statusMessage: 'Rival not found' })
  }
  setResponseHeader(event, 'Cache-Control', 'private, no-store')
  return page
})
