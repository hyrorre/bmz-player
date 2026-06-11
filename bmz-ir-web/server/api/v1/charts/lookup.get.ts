import { serverSupabaseServiceRole } from '#supabase/server'
import { getQuery } from 'h3'

import type { Database } from '../../../../shared/types/database.types'
import { lookupChartSha256ByMd5 } from '../../../services/charts'
import { requireHex } from '../../../services/ir'

export default defineEventHandler(async (event) => {
  const query = getQuery(event)
  const md5 = typeof query.md5 === 'string' ? query.md5.trim().toLowerCase() : ''
  if (!md5) {
    throw createError({ statusCode: 400, statusMessage: 'md5 is required' })
  }
  try {
    requireHex(md5, 32, 'md5')
  } catch {
    throw createError({ statusCode: 400, statusMessage: 'md5 must be lowercase hex length 32' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const sha256 = await lookupChartSha256ByMd5(db, md5)
  if (!sha256) {
    throw createError({ statusCode: 404, statusMessage: 'Chart not found' })
  }
  return { sha256 }
})
