import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../../utils/auth'
import type { Database } from '../../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const keyId = getRouterParam(event, 'id')
  if (!keyId) {
    throw createError({ statusCode: 400, statusMessage: 'device key id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data, error } = await db
    .from('device_keys')
    .update({ revoked_at: new Date().toISOString() })
    .eq('id', keyId)
    .eq('player_id', user.id)
    .is('revoked_at', null)
    .select('id')
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  if (!data) {
    throw createError({ statusCode: 404, statusMessage: 'Device key not found or already revoked' })
  }
  return { revoked: true, id: data.id }
})
