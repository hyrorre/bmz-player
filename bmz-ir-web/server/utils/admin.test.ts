import { describe, expect, test } from 'bun:test'
import { isAdminUser, parseAdminUserIds } from './admin'

describe('IR administrator allowlist', () => {
  test('normalizes whitespace, empty entries, and duplicates', () => {
    expect([...parseAdminUserIds(' user-a, user-b ,,user-a ')]).toEqual(['user-a', 'user-b'])
  })

  test('fails closed for missing or empty configuration', () => {
    expect(isAdminUser('user-a', undefined)).toBe(false)
    expect(isAdminUser('user-a', '')).toBe(false)
  })

  test('matches only a complete configured user id', () => {
    expect(isAdminUser('user-a', 'user-a,user-b')).toBe(true)
    expect(isAdminUser('user', 'user-a,user-b')).toBe(false)
  })
})
