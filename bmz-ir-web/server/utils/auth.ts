import type { H3Event } from 'h3'
import { getHeader, createError } from 'h3'
import { moderationHttpError } from '../services/moderation/enforcement'
import { findUserByAccessToken, findUserByWebSession } from './auth_tokens'

export interface IrUser {
  id: string
  email?: string
}

/**
 * BMZ デスクトップクライアントは Authorization: Bearer <access_token> を送る。
 * ブラウザは nuxt-auth-utils の cookie セッションを使う。両方を解決する。
 *
 * BAN済みユーザーは未ログインとして扱う。公開ページの閲覧は妨げない。
 */
export async function resolveIrUser(event: H3Event): Promise<IrUser | null> {
  const resolved = await resolveIrUserState(event)
  return resolved === 'banned' ? null : resolved
}

async function resolveIrUserState(event: H3Event): Promise<IrUser | 'banned' | null> {
  const token = getBearerToken(event)
  if (token) {
    const user = await findUserByAccessToken(token)
    if (!user) {
      return null
    }
    if (user.moderationStatus === 'banned') {
      return 'banned'
    }
    return { id: user.id, email: user.email }
  }

  const session = await getUserSession(event)
  const user = session.user as { id?: string; email?: string } | undefined
  const sessionGroupId = session.secure?.sessionGroupId
  if (!user?.id || !sessionGroupId) {
    return null
  }
  return findUserByWebSession(user.id, sessionGroupId)
}

export function getBearerToken(event: H3Event): string | null {
  const header = getHeader(event, 'authorization')
  if (!header?.toLowerCase().startsWith('bearer ')) {
    return null
  }

  return header.slice('bearer '.length).trim() || null
}

/** 認証が必要なAPI用。BAN済みユーザーには ACCOUNT_BANNED の 403 を返す。 */
export async function requireIrUser(event: H3Event): Promise<IrUser> {
  const user = await resolveIrUserState(event)
  if (user === 'banned') {
    throw moderationHttpError('ACCOUNT_BANNED')
  }
  if (!user) {
    throw createError({ statusCode: 401, statusMessage: 'Authentication required' })
  }
  return user
}
