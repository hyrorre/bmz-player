import { describe, expect, test } from 'bun:test'
import { irProviderKeyForHost } from './provider_key'

describe('irProviderKeyForHost', () => {
  test('returns bmz-dev for local hosts', () => {
    expect(irProviderKeyForHost('localhost:3000')).toBe('bmz-dev')
    expect(irProviderKeyForHost('127.0.0.1:3000')).toBe('bmz-dev')
    expect(irProviderKeyForHost('[::1]:3000')).toBe('bmz-dev')
  })

  test('returns bmz for production hosts', () => {
    expect(irProviderKeyForHost('bmz-player.hyrorre.workers.dev')).toBe('bmz')
    expect(irProviderKeyForHost(undefined)).toBe('bmz')
  })
})
