import { serverSupabaseServiceRole, serverSupabaseUser } from '#supabase/server'
import { readBody } from 'h3'
import type { Database } from '../../../shared/types/database.types'

interface RivalRequestBody {
  target_player_id?: string
  action?: 'add' | 'remove'
}

export default defineEventHandler(async (event) => {
  const user = await serverSupabaseUser(event)
  if (!user) {
    throw createError({ statusCode: 401, statusMessage: 'Authentication required' })
  }

  const body = (await readBody(event)) as RivalRequestBody
  const targetPlayerId = body.target_player_id
  if (!targetPlayerId || targetPlayerId === user.id) {
    throw createError({ statusCode: 400, statusMessage: 'valid target_player_id is required' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  if (body.action === 'remove') {
    const { error } = await db
      .from('rival_relationships')
      .delete()
      .eq('owner_player_id', user.id)
      .eq('target_player_id', targetPlayerId)
      .eq('relation_type', 'rival')
    if (error) {
      throw createError({ statusCode: 500, statusMessage: error.message })
    }
    return { removed: true }
  }

  const { error } = await db.from('rival_relationships').upsert(
    {
      owner_player_id: user.id,
      target_player_id: targetPlayerId,
      relation_type: 'rival',
    },
    { onConflict: 'owner_player_id,target_player_id,relation_type' },
  )
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }
  return { added: true }
})
