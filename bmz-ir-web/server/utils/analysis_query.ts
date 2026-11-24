import { createError, type H3Event } from 'h3'
import type { IrDistributionMode } from '../../shared/types/ir'
import { requireIrUser } from './auth'

export function analysisQueryText(value: unknown, name: string): string | undefined {
  if (value === undefined) return undefined
  if (typeof value !== 'string' || !value.trim()) {
    throw createError({ statusCode: 400, statusMessage: `${name} must be a non-empty value` })
  }
  return value.trim()
}

export async function analysisPlayerId(event: H3Event, value: unknown) {
  return analysisQueryText(value, 'player') ?? (await requireIrUser(event)).id
}

export function distributionMode(value: unknown): IrDistributionMode {
  if (value === undefined || value === 'lamp') return 'lamp'
  if (value === 'score') return 'score'
  throw createError({ statusCode: 400, statusMessage: 'mode must be lamp or score' })
}
