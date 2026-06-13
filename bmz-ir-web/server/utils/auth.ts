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
  const token = getBearerToken(event)
  if (token) {
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

export function getBearerToken(event: H3Event): string | null {
  const header = getHeader(event, 'authorization')
  if (!header?.toLowerCase().startsWith('bearer ')) {
    return null
  }

  return header.slice('bearer '.length).trim() || null
}

export async function requireIrUser(event: H3Event): Promise<IrUser> {
  const user = await resolveIrUser(event)
  if (!user) {
    throw createError({ statusCode: 401, statusMessage: 'Authentication required' })
  }
  return user
}
