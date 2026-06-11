import { readBody, createError } from 'h3'
import { createClient } from '@supabase/supabase-js'
import type { Database } from '../../../../shared/types/database.types'

interface RefreshBody {
  refresh_token?: string
}

export default defineEventHandler(async (event) => {
  const body = (await readBody(event)) as RefreshBody
  if (!body?.refresh_token) {
    throw createError({ statusCode: 400, statusMessage: 'refresh_token is required' })
  }

  const { url, key } = useRuntimeConfig(event).public.supabase
  const auth = createClient<Database>(url, key, {
    auth: { persistSession: false, autoRefreshToken: false },
  })
  const { data, error } = await auth.auth.refreshSession({ refresh_token: body.refresh_token })
  if (error || !data.session || !data.user) {
    throw createError({ statusCode: 401, statusMessage: error?.message ?? 'Invalid refresh token' })
  }

  return {
    access_token: data.session.access_token,
    refresh_token: data.session.refresh_token,
    expires_at: data.session.expires_at ?? null,
    player: {
      id: data.user.id,
      email: data.user.email ?? null,
    },
  }
})
