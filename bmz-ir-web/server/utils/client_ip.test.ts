import { describe, expect, test } from 'bun:test'
import type { H3Event } from 'h3'
import { rateLimitClientIp } from './client_ip'

function event(headers: Record<string, string>, context = {}): H3Event {
  return {
    context,
    node: { req: { headers, socket: { remoteAddress: '203.0.113.9' } } },
  } as unknown as H3Event
}

describe('trusted rate-limit address', () => {
  test('ignores spoofed forwarded headers on a Node server', () => {
    for (const forged of ['198.51.100.1', '198.51.100.2']) {
      expect(
        rateLimitClientIp(event({ 'x-forwarded-for': forged, 'cf-connecting-ip': forged })),
      ).toBe('203.0.113.9')
    }
  })
  test('uses Cloudflare address, not the user-supplied XFF prefix', () => {
    for (const forged of ['198.51.100.1', '198.51.100.2']) {
      expect(
        rateLimitClientIp(
          event(
            { 'x-forwarded-for': `${forged}, 203.0.113.9`, 'cf-connecting-ip': '203.0.113.9' },
            { _platform: { cloudflare: { request: {} } } },
          ),
        ),
      ).toBe('203.0.113.9')
    }
  })
  test('validates addresses and shares an unknown bucket when platform data is missing', () => {
    const context = { cloudflare: { request: {} } }
    expect(rateLimitClientIp(event({}, context))).toBe('unknown')
    expect(rateLimitClientIp(event({ 'cf-connecting-ip': 'anything' }, context))).toBe('unknown')
    expect(rateLimitClientIp(event({ 'cf-connecting-ip': '2001:db8::1' }, context))).toBe(
      '2001:db8::1',
    )
  })
})
