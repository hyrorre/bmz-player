import { getQuery } from 'h3'
import { getRanking, parseRankingQuery } from '../../../../services/ir'
import { resolveIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const sha256 = getRouterParam(event, 'sha256')
  if (!sha256) {
    throw createError({ statusCode: 400, statusMessage: 'chart sha256 is required' })
  }
  const user = await resolveIrUser(event)
  return getRanking(user, sha256, parseRankingQuery(getQuery(event)))
})
