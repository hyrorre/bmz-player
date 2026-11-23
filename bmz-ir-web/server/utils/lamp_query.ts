import { createError, type H3Event } from 'h3'
import type { IrDistributionMode } from '../../shared/types/ir'
import { requireIrUser } from './auth'

export function lampQueryText(value: unknown, name: string): string | undefined {
  if (value === undefined) return undefined
  if (typeof value !== 'string' || !value.trim()) {
    throw createError({ statusCode: 400, statusMessage: `${name} must be a non-empty value` })
  }
  return value.trim()
}

export async function lampPlayerId(event: H3Event, value: unknown) {
  return lampQueryText(value, 'player') ?? (await requireIrUser(event)).id
}

export function distributionMode(value: unknown): IrDistributionMode {
  if (value === undefined || value === 'lamp') return 'lamp'
  if (value === 'score') return 'score'
  throw createError({ statusCode: 400, statusMessage: 'mode must be lamp or score' })
}
