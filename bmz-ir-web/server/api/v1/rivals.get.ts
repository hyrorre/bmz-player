import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../utils/auth'
import type { Database } from '../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)

  const db = serverSupabaseServiceRole<Database>(event)
  const { data, error } = await db
    .from('rival_relationships')
    .select('target_player_id, relation_type, created_at')
    .eq('owner_player_id', user.id)
    .eq('relation_type', 'rival')
    .order('created_at', { ascending: false })

  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }

  const targetIds = (data ?? []).map((row) => row.target_player_id)
  const { data: profiles, error: profilesError } =
    targetIds.length > 0
      ? await db.from('profiles').select('id, display_name, bio').in('id', targetIds)
      : { data: [], error: null }

  if (profilesError) {
    throw createError({ statusCode: 500, statusMessage: profilesError.message })
  }

  const profileMap = new Map((profiles ?? []).map((profile) => [profile.id, profile]))
  return {
    rivals: (data ?? []).map((row) => ({
      player_id: row.target_player_id,
      relation_type: row.relation_type,
      created_at: row.created_at,
      profile: profileMap.get(row.target_player_id) ?? null,
    })),
  }
})
