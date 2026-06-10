import { serverSupabaseServiceRole } from '#supabase/server'
import { requireIrUser } from '../../utils/auth'
import type { Database } from '../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const db = serverSupabaseServiceRole<Database>(event)
  const { data: profile, error } = await db
    .from('profiles')
    .select('display_name, bio')
    .eq('id', user.id)
    .maybeSingle()
  if (error) {
    throw createError({ statusCode: 500, statusMessage: error.message })
  }

  return {
    player: {
      id: user.id,
      email: user.email ?? null,
      display_name: profile?.display_name ?? null,
      bio: profile?.bio ?? null,
    },
  }
})
