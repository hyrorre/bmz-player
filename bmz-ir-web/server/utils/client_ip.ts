import { isIP } from 'node:net'
import { getHeader, type H3Event } from 'h3'

/** Trust Cloudflare's address only inside the Cloudflare runtime. Forwarded
 * headers supplied to a Node/dev server must never choose a rate-limit bucket. */
export function rateLimitClientIp(event: H3Event): string {
  const cloudflare = event.context.cloudflare ?? event.context._platform?.cloudflare
  const candidate = cloudflare?.request
    ? getHeader(event, 'cf-connecting-ip')
    : event.node.req.socket?.remoteAddress
  return typeof candidate === 'string' && isIP(candidate) ? candidate : 'unknown'
}
