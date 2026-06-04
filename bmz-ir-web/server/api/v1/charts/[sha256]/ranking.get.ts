import { serverSupabaseServiceRole, serverSupabaseUser } from '#supabase/server'
import { getQuery } from 'h3'
import { getRanking, parseRankingQuery } from '../../../../services/ir'
import type { Database } from '../../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const sha256 = getRouterParam(event, 'sha256')
  if (!sha256) {
    throw createError({ statusCode: 400, statusMessage: 'chart sha256 is required' })
  }
  const user = await serverSupabaseUser(event)
  const db = serverSupabaseServiceRole<Database>(event)
  return getRanking(db, user, sha256, parseRankingQuery(getQuery(event)))
})
