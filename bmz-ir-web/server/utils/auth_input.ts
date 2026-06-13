import { createError } from 'h3'

export function normalizeEmail(value: unknown): string {
  return typeof value === 'string' ? value.trim().toLowerCase() : ''
}

export function normalizeDisplayName(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}

export function readPassword(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

export function requirePassword(value: unknown, fieldName = 'password'): string {
  const password = readPassword(value)
  if (!password) {
    throw createError({ statusCode: 400, statusMessage: `${fieldName} is required` })
  }
  if (password.length < 8) {
    throw createError({ statusCode: 400, statusMessage: `${fieldName} must be at least 8 characters` })
  }
  return password
}
