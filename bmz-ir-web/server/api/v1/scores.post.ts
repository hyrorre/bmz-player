import { getQuery, readBody } from 'h3'
import { parseRankingScope, submitScore, validateScoreSubmission } from '../../services/ir'
import { requireIrUser } from '../../utils/auth'
import { checkUserRateLimit } from '../../utils/rate_limit'
import type { IrRankingScope } from '../../../shared/types/ir'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  // オフライン分の一括 sync (数十件程度) は許容しつつ書き込み spam を抑える。
  await checkUserRateLimit(event, 'score_submit', user.id, { user: 300, ip: 600 })

  const body = await readBody(event)
  const payload = validateScoreSubmission(body)
  const query = getQuery(event)
  const include = String(query.include ?? '')
  const rankingLimit = Math.max(1, Math.min(200, Number(query.ranking_limit ?? 100) || 100))
  const rankingScopes = include.split(',').includes('rankings')
    ? parseRankingScopes(String(query.ranking_scopes ?? 'global'))
    : []

  return submitScore(user, payload, rankingScopes, rankingLimit)
})

function parseRankingScopes(value: string): IrRankingScope[] {
  const scopes = value
    .split(',')
    .map((scope) => scope.trim())
    .filter(Boolean)
  return scopes.map((scope) => parseRankingScope(scope))
}
