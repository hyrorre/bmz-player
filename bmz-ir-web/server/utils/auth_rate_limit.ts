import { getRequestIP, type H3Event } from 'h3'
import { incrementRateLimit, pruneExpiredRateLimits } from './rate_limit'

type AuthRateLimitAction = 'login' | 'register'

const LIMITS: Record<AuthRateLimitAction, { email: number; ip: number }> = {
  login: { email: 8, ip: 40 },
  register: { email: 3, ip: 20 },
}

export async function checkAuthRateLimit(
  event: H3Event,
  action: AuthRateLimitAction,
  email: string,
  now = Date.now(),
) {
  const message = 'Too many authentication attempts'
  const ip = getRequestIP(event, { xForwardedFor: true }) ?? 'unknown'
  await Promise.all([
    incrementRateLimit(action, 'email', email, LIMITS[action].email, now, message, event),
    incrementRateLimit(action, 'ip', ip, LIMITS[action].ip, now, message, event),
  ])
  await pruneExpiredRateLimits(now)
}
