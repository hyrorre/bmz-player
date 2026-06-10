import { readBody, createError } from 'h3'
import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../utils/auth'
import type { Database } from '../../../shared/types/database.types'

interface DeviceKeyBody {
  public_key?: string
  algorithm?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as DeviceKeyBody
  const publicKey = body?.public_key?.toLowerCase() ?? ''
  if (!/^[0-9a-f]{64}$/.test(publicKey)) {
    throw createError({ statusCode: 400, statusMessage: 'public_key must be 32 bytes hex' })
  }
  if ((body.algorithm ?? 'ed25519') !== 'ed25519') {
    throw createError({ statusCode: 400, statusMessage: 'unsupported algorithm' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: existing, error: existingError } = await db
    .from('device_keys')
    .select('id')
    .eq('player_id', user.id)
    .eq('public_key', publicKey)
    .is('revoked_at', null)
    .maybeSingle()
  if (existingError) {
    throw createError({ statusCode: 500, statusMessage: existingError.message })
  }
  if (existing) {
    return { id: existing.id, created: false }
  }

  const { data: inserted, error } = await db
    .from('device_keys')
    .insert({ player_id: user.id, public_key: publicKey, algorithm: 'ed25519' })
    .select('id')
    .single()
  if (error || !inserted) {
    throw createError({ statusCode: 500, statusMessage: error?.message ?? 'insert failed' })
  }
  return { id: inserted.id, created: true }
})
