import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../../utils/auth'
import type { Database } from '../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const db = serverSupabaseServiceRole<Database>(event)
  const { data, error } = await db
    .from('device_keys')
    .select('id, public_key, algorithm, revoked_at, created_at')
    .eq('player_id', user.id)
    .order('created_at', { ascending: false })
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return { device_keys: data ?? [] }
})
