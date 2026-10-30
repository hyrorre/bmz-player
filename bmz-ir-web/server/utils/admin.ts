import type { H3Event } from 'h3'
import { createError } from 'h3'
import { requireIrUser, type IrUser } from './auth'

export function parseAdminUserIds(value: unknown): Set<string> {
  if (typeof value !== 'string') return new Set()
  return new Set(
    value
      .split(',')
      .map((id) => id.trim())
      .filter(Boolean),
  )
}

export function isAdminUser(userId: string, configuredIds: unknown): boolean {
  return parseAdminUserIds(configuredIds).has(userId)
}

export function configuredAdminUserIds(event: H3Event): Set<string> {
  return parseAdminUserIds(useRuntimeConfig(event).ir?.adminUserIds)
}

/**
 * 管理者API用。管理者はサーバー設定の adminUserIds だけで判定し、リクエスト内の値は使わない。
 * 設定が空の環境では全員を拒否する。
 */
export async function requireIrAdmin(event: H3Event): Promise<IrUser> {
  const user = await requireIrUser(event)
  if (!configuredAdminUserIds(event).has(user.id)) {
    throw createError({ statusCode: 403, statusMessage: 'Administrator access required' })
  }
  return user
}
