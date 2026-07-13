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

export async function requireIrAdmin(event: H3Event): Promise<IrUser> {
  const user = await requireIrUser(event)
  const configuredIds = useRuntimeConfig(event).ir?.adminUserIds
  if (!isAdminUser(user.id, configuredIds)) {
    throw createError({ statusCode: 403, statusMessage: 'Administrator access required' })
  }
  return user
}
