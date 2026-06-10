import type { H3Event } from 'h3'
import { getHeader, createError } from 'h3'
import { serverSupabaseClient, serverSupabaseServiceRole } from '#supabase/server'
import type { Database } from '../../shared/types/database.types'

export interface IrUser {
  id: string
  email?: string
}

/**
 * BMZ デスクトップクライアントは Authorization: Bearer <access_token> を送る。
 * ブラウザは Supabase の cookie セッションを使う。両方を解決する。
 */
export async function resolveIrUser(event: H3Event): Promise<IrUser | null> {
  const header = getHeader(event, 'authorization')
  if (header && header.toLowerCase().startsWith('bearer ')) {
    const token = header.slice('bearer '.length).trim()
    if (!token) {
      return null
    }
    const db = serverSupabaseServiceRole<Database>(event)
    const { data, error } = await db.auth.getUser(token)
    if (error || !data.user) {
      return null
    }
    return { id: data.user.id, email: data.user.email ?? undefined }
  }

  const client = await serverSupabaseClient<Database>(event)
  const { data, error } = await client.auth.getUser()
  if (error || !data.user) {
    return null
  }
  return { id: data.user.id, email: data.user.email ?? undefined }
}

export async function requireIrUser(event: H3Event): Promise<IrUser> {
  const user = await resolveIrUser(event)
  if (!user) {
    throw createError({ statusCode: 401, statusMessage: 'Authentication required' })
  }
  return user
}
