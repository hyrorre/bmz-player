import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery, readBody } from 'h3'
import {
  parseRankingQuery,
  submitScore,
  validateScoreSubmission,
} from '../../services/ir'
import { requireIrUser } from '../../utils/auth'
import type { IrRankingScope } from '../../../shared/types/ir'
import type { Database } from '../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)

  const body = await readBody(event)
  const payload = validateScoreSubmission(body)
  const query = getQuery(event)
  const include = String(query.include ?? '')
  const rankingLimit = Math.max(1, Math.min(200, Number(query.ranking_limit ?? 100) || 100))
  const rankingScopes = include
    .split(',')
    .includes('rankings')
    ? parseRankingScopes(String(query.ranking_scopes ?? 'global'))
    : []

  const db = serverSupabaseServiceRole<Database>(event)
  return submitScore(db, user, payload, rankingScopes, rankingLimit)
})

function parseRankingScopes(value: string): IrRankingScope[] {
  const scopes = value
    .split(',')
    .map((scope) => scope.trim())
    .filter(Boolean)
  return scopes.map((scope) => parseRankingQuery({ scope }).scope)
}
