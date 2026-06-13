import { createHash } from 'node:crypto'
import { and, eq, lt } from 'drizzle-orm'
import { createError, getRequestIP, type H3Event } from 'h3'
import { db, schema } from 'hub:db'

type AuthRateLimitAction = 'login' | 'register'

const WINDOW_MS = 15 * 60 * 1000
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
  const windowStart = Math.floor(now / WINDOW_MS) * WINDOW_MS
  const ip = getRequestIP(event, { xForwardedFor: true }) ?? 'unknown'
  await Promise.all([
    incrementAuthRateLimit(action, 'email', email, windowStart, now, LIMITS[action].email),
    incrementAuthRateLimit(action, 'ip', ip, windowStart, now, LIMITS[action].ip),
  ])

  await db
    .delete(schema.authRateLimits)
    .where(lt(schema.authRateLimits.updatedAt, new Date(now - WINDOW_MS * 4)))
}

async function incrementAuthRateLimit(
  action: AuthRateLimitAction,
  scope: 'email' | 'ip',
  scopeValue: string,
  windowStartMs: number,
  nowMs: number,
  limit: number,
) {
  const scopeHash = hashScope(scopeValue)
  const windowStart = new Date(windowStartMs)
  const now = new Date(nowMs)

  const existing = await db.query.authRateLimits.findFirst({
    columns: { attempts: true },
    where: and(
      eq(schema.authRateLimits.action, action),
      eq(schema.authRateLimits.scope, scope),
      eq(schema.authRateLimits.scopeHash, scopeHash),
      eq(schema.authRateLimits.windowStart, windowStart),
    ),
  })
  const attempts = (existing?.attempts ?? 0) + 1
  if (attempts > limit) {
    throw createError({ statusCode: 429, statusMessage: 'Too many authentication attempts' })
  }

  if (existing) {
    await db
      .update(schema.authRateLimits)
      .set({ attempts, updatedAt: now })
      .where(
        and(
          eq(schema.authRateLimits.action, action),
          eq(schema.authRateLimits.scope, scope),
          eq(schema.authRateLimits.scopeHash, scopeHash),
          eq(schema.authRateLimits.windowStart, windowStart),
        ),
      )
    return
  }

  await db.insert(schema.authRateLimits).values({
    action,
    scope,
    scopeHash,
    windowStart,
    attempts,
    updatedAt: now,
  })
}

function hashScope(value: string): string {
  return createHash('sha256').update(value).digest('hex')
}
