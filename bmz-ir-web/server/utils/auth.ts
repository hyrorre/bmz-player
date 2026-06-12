import type { H3Event } from 'h3'
import { getHeader, createError } from 'h3'
import { findUserByAccessToken } from './auth_tokens'

export interface IrUser {
  id: string
  email?: string
}

/**
 * BMZ デスクトップクライアントは Authorization: Bearer <access_token> を送る。
 * ブラウザは nuxt-auth-utils の cookie セッションを使う。両方を解決する。
 */
export async function resolveIrUser(event: H3Event): Promise<IrUser | null> {
  const header = getHeader(event, 'authorization')
  if (header && header.toLowerCase().startsWith('bearer ')) {
    const token = header.slice('bearer '.length).trim()
    if (!token) {
      return null
    }
    const user = await findUserByAccessToken(token)
    if (!user) {
      return null
    }
    return { id: user.id, email: user.email }
  }

  const session = await getUserSession(event)
  const user = session.user as { id?: string; email?: string } | undefined
  if (!user?.id) {
    return null
  }
  return { id: user.id, email: user.email }
}

export async function requireIrUser(event: H3Event): Promise<IrUser> {
  const user = await resolveIrUser(event)
  if (!user) {
    throw createError({ statusCode: 401, statusMessage: 'Authentication required' })
  }
  return user
}
