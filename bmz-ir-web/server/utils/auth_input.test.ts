import { describe, expect, test } from 'bun:test'
import {
  MAX_DISPLAY_NAME_CHARS,
  normalizeDisplayName,
  normalizeEmail,
  readPassword,
  requirePassword,
  requireDisplayName,
} from './auth_input'

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

  test('requires 1 to 64 trimmed display-name characters without controls', () => {
    expect(requireDisplayName(' x ')).toBe('x')
    expect(requireDisplayName('あ'.repeat(MAX_DISPLAY_NAME_CHARS))).toHaveLength(
      MAX_DISPLAY_NAME_CHARS,
    )
    for (const value of [
      '',
      '  ',
      undefined,
      'x'.repeat(MAX_DISPLAY_NAME_CHARS + 1),
      'a\nb',
      'a\x00b',
      'a\x7fb',
      'a\x85b',
    ]) {
      try {
        requireDisplayName(value)
        throw new Error('expected a validation error')
      } catch (error) {
        expect(error).toMatchObject({ statusCode: 400 })
      }
    }
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
