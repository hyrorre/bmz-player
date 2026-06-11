import type { SupabaseClient } from '@supabase/supabase-js'

import type { Database } from '../../shared/types/database.types'

export async function lookupChartSha256ByMd5(
  db: SupabaseClient<Database>,
  md5: string,
): Promise<string | null> {
  const { data, error } = await db.from('charts').select('sha256').eq('md5', md5).maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return data?.sha256 ?? null
}
