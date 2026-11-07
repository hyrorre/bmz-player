import { createError } from 'h3'

export const MAX_DISPLAY_NAME_CHARS = 64

export function normalizeEmail(value: unknown): string {
  return typeof value === 'string' ? value.trim().toLowerCase() : ''
}

export function normalizeDisplayName(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}

export function requireDisplayName(value: unknown): string {
  const name = normalizeDisplayName(value)
  if (!name) throw createError({ statusCode: 400, statusMessage: 'display_name is required' })
  if (name.length > MAX_DISPLAY_NAME_CHARS) {
    throw createError({ statusCode: 400, statusMessage: 'display_name is too long' })
  }
  if (/\p{Cc}/u.test(name)) {
    throw createError({
      statusCode: 400,
      statusMessage: 'display_name contains control characters',
    })
  }
  return name
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
    throw createError({
      statusCode: 400,
      statusMessage: `${fieldName} must be at least 8 characters`,
    })
  }
  return password
}
