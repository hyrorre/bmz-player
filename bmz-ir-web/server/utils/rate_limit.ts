import { createHash } from 'node:crypto'
import { lt, sql } from 'drizzle-orm'
import { createError, getRequestIP, setHeader, type H3Event } from 'h3'
import { db, schema } from 'hub:db'

export type RateLimitAction = 'login' | 'register' | 'score_submit' | 'refresh' | 'replay_upload'
export type RateLimitScope = 'email' | 'ip' | 'user'

export const RATE_LIMIT_WINDOW_MS = 15 * 60 * 1000
export const SCORE_SUBMIT_RATE_LIMIT = { user: 1500, ip: 3000 } as const
export const REPLAY_UPLOAD_RATE_LIMIT = { user: 900, ip: 1800 } as const

/**
 * 認証済み API 向けのレート制限。user id と IP の両方で数える。
 *
 * ウィンドウはオフライン分をまとめて sync する正規クライアントの
 * バースト (数十〜百件程度) を許容しつつ、書き込み spam を抑える値にする。
 */
export async function checkUserRateLimit(
  event: H3Event,
  action: RateLimitAction,
  userId: string,
  limits: { user: number; ip: number },
  now = Date.now(),
) {
  const ip = getRequestIP(event, { xForwardedFor: true }) ?? 'unknown'
  await Promise.all([
    incrementRateLimit(action, 'user', userId, limits.user, now, 'Too many requests', event),
    incrementRateLimit(action, 'ip', ip, limits.ip, now, 'Too many requests', event),
  ])
  await pruneExpiredRateLimits(now)
}

/** 認証前のエンドポイント (refresh 等) 向けの IP のみのレート制限。 */
export async function checkIpRateLimit(
  event: H3Event,
  action: RateLimitAction,
  limit: number,
  now = Date.now(),
) {
  const ip = getRequestIP(event, { xForwardedFor: true }) ?? 'unknown'
  await incrementRateLimit(action, 'ip', ip, limit, now, 'Too many requests', event)
  await pruneExpiredRateLimits(now)
}

export async function incrementRateLimit(
  action: RateLimitAction,
  scope: RateLimitScope,
  scopeValue: string,
  limit: number,
  nowMs: number,
  statusMessage = 'Too many requests',
  event?: H3Event,
) {
  const scopeHash = hashScope(scopeValue)
  const windowStart = rateLimitWindowStart(nowMs)
  const now = new Date(nowMs)

  const attempts = await incrementRateLimitAttempt(db, {
    action,
    scope,
    scopeHash,
    windowStart,
    now,
  })
  if (attempts > limit) {
    if (event) {
      const retryAfterSeconds = retryAfterFromWindowStart(windowStart, nowMs)
      setHeader(event, 'Retry-After', retryAfterSeconds)
    }
    throw createError({ statusCode: 429, statusMessage })
  }
}

async function incrementRateLimitAttempt(
  database: Pick<typeof db, 'insert'>,
  values: {
    action: RateLimitAction
    scope: RateLimitScope
    scopeHash: string
    windowStart: Date
    now: Date
  },
): Promise<number> {
  const updated = await database
    .insert(schema.authRateLimits)
    .values({
      action: values.action,
      scope: values.scope,
      scopeHash: values.scopeHash,
      windowStart: values.windowStart,
      attempts: 1,
      updatedAt: values.now,
    })
    .onConflictDoUpdate({
      target: [
        schema.authRateLimits.action,
        schema.authRateLimits.scope,
        schema.authRateLimits.scopeHash,
        schema.authRateLimits.windowStart,
      ],
      set: {
        attempts: sql`${schema.authRateLimits.attempts} + ${1}`,
        updatedAt: values.now,
      },
    })
    .returning({ attempts: schema.authRateLimits.attempts })
    .get()
  return updated.attempts
}

export async function pruneExpiredRateLimits(now: number) {
  await db
    .delete(schema.authRateLimits)
    .where(lt(schema.authRateLimits.updatedAt, new Date(now - RATE_LIMIT_WINDOW_MS * 4)))
}

function hashScope(value: string): string {
  return createHash('sha256').update(value).digest('hex')
}

function rateLimitWindowStart(nowMs: number): Date {
  return new Date(Math.floor(nowMs / RATE_LIMIT_WINDOW_MS) * RATE_LIMIT_WINDOW_MS)
}

function retryAfterFromWindowStart(windowStart: Date, nowMs: number): number {
  const windowEndMs = windowStart.getTime() + RATE_LIMIT_WINDOW_MS
  return Math.max(1, Math.ceil((windowEndMs - nowMs) / 1000))
}

export const __test = {
  incrementRateLimitAttempt,
  rateLimitWindowStart,
  retryAfterFromWindowStart,
}
