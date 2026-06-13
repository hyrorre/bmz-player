import { describe, expect, test } from 'bun:test'
import { normalizeDisplayName, normalizeEmail, readPassword, requirePassword } from './auth_input'

describe('auth input helpers', () => {
  test('normalizes email values', () => {
    expect(normalizeEmail(' USER@Example.COM ')).toBe('user@example.com')
    expect(normalizeEmail(null)).toBe('')
    expect(normalizeEmail(123)).toBe('')
  })

  test('normalizes display names without changing case', () => {
    expect(normalizeDisplayName('  Player One  ')).toBe('Player One')
    expect(normalizeDisplayName(undefined)).toBe('')
  })

  test('reads password strings only', () => {
    expect(readPassword('secret')).toBe('secret')
    expect(readPassword(false)).toBe('')
  })

  test('requires non-empty passwords with the minimum length', () => {
    expect(requirePassword('12345678')).toBe('12345678')
    expect(() => requirePassword('')).toThrow('password is required')
    expect(() => requirePassword('1234567')).toThrow('password must be at least 8 characters')
    expect(() => requirePassword('1234567', 'current_password')).toThrow(
      'current_password must be at least 8 characters',
    )
  })
})
