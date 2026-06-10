import { readBody, createError } from 'h3'
import { createClient } from '@supabase/supabase-js'
import { serverSupabaseServiceRole } from '#supabase/server'
import type { Database } from '../../../../shared/types/database.types'

interface LoginBody {
  email?: string
  password?: string
}

export default defineEventHandler(async (event) => {
  const body = (await readBody(event)) as LoginBody
  if (!body?.email || !body?.password) {
    throw createError({ statusCode: 400, statusMessage: 'email and password are required' })
  }

  const { url, key } = useRuntimeConfig(event).public.supabase
  const auth = createClient<Database>(url, key, {
    auth: { persistSession: false, autoRefreshToken: false },
  })
  const { data, error } = await auth.auth.signInWithPassword({
    email: body.email,
    password: body.password,
  })
  if (error || !data.session || !data.user) {
    throw createError({ statusCode: 401, statusMessage: error?.message ?? 'Invalid credentials' })
  }

  const db = serverSupabaseServiceRole<Database>(event)
  const { data: profile } = await db
    .from('profiles')
    .select('display_name')
    .eq('id', data.user.id)
    .maybeSingle()

  return {
    access_token: data.session.access_token,
    refresh_token: data.session.refresh_token,
    expires_at: data.session.expires_at ?? null,
    player: {
      id: data.user.id,
      email: data.user.email ?? null,
      display_name: profile?.display_name ?? null,
    },
  }
})
