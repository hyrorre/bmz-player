import { createHash, randomBytes } from 'node:crypto'
import { and, eq, gt, isNull } from 'drizzle-orm'
import { db, schema } from 'hub:db'

const ACCESS_TOKEN_TTL_SECONDS = 60 * 60
const REFRESH_TOKEN_TTL_SECONDS = 60 * 60 * 24 * 30
type SessionKind = 'access' | 'refresh'
type RevocationReason = 'logout' | 'rotated' | 'password_changed' | 'reuse_detected' | 'admin'

export interface AuthTokenPair {
  accessToken: string
  refreshToken: string
  accessExpiresAt: number
}

export function hashToken(token: string) {
  return createHash('sha256').update(token).digest('hex')
}

export async function createAuthTokens(userId: string, now = Date.now()): Promise<AuthTokenPair> {
  const accessToken = randomToken()
  const refreshToken = randomToken()
  const accessExpiresAt = Math.floor(now / 1000) + ACCESS_TOKEN_TTL_SECONDS
  const refreshExpiresAt = now + REFRESH_TOKEN_TTL_SECONDS * 1000

  await db.insert(schema.sessions).values([
    {
      tokenHash: hashToken(accessToken),
      userId,
      kind: 'access',
      expiresAt: new Date(accessExpiresAt * 1000),
    },
    {
      tokenHash: hashToken(refreshToken),
      userId,
      kind: 'refresh',
      expiresAt: new Date(refreshExpiresAt),
    },
  ])

  return { accessToken, refreshToken, accessExpiresAt }
}

export async function findUserByAccessToken(token: string, now = Date.now()) {
  const rows = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      displayName: schema.profiles.displayName,
    })
    .from(schema.sessions)
    .innerJoin(schema.users, eq(schema.sessions.userId, schema.users.id))
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .where(
      and(
        eq(schema.sessions.tokenHash, hashToken(token)),
        eq(schema.sessions.kind, 'access'),
        isNull(schema.sessions.revokedAt),
        gt(schema.sessions.expiresAt, new Date(now)),
      ),
    )
    .limit(1)

  return rows[0] ?? null
}

export async function rotateRefreshToken(token: string, now = Date.now()) {
  const session = await db.query.sessions.findFirst({
    columns: {
      tokenHash: true,
      userId: true,
      expiresAt: true,
      revokedAt: true,
    },
    where: and(
      eq(schema.sessions.tokenHash, hashToken(token)),
      eq(schema.sessions.kind, 'refresh'),
    ),
  })

  if (!session) {
    return null
  }

  if (session.revokedAt) {
    await revokeUserSessions(session.userId, 'reuse_detected', now)
    return { reuseDetected: true as const }
  }

  if (session.expiresAt <= new Date(now)) {
    return null
  }

  const rows = await db
    .select({
      email: schema.users.email,
      displayName: schema.profiles.displayName,
    })
    .from(schema.users)
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .where(eq(schema.users.id, session.userId))
    .limit(1)

  const user = rows[0]
  if (!user) {
    return null
  }

  await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now), revokedReason: 'rotated' })
    .where(eq(schema.sessions.tokenHash, session.tokenHash))

  return {
    tokens: await createAuthTokens(session.userId, now),
    user: {
      id: session.userId,
      email: user.email,
      displayName: user.displayName ?? '',
    },
  }
}

export async function revokeToken(
  token: string,
  kind?: SessionKind,
  reason: RevocationReason = 'logout',
  now = Date.now(),
) {
  const filters = [
    eq(schema.sessions.tokenHash, hashToken(token)),
    isNull(schema.sessions.revokedAt),
  ]
  if (kind) {
    filters.push(eq(schema.sessions.kind, kind))
  }

  await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now), revokedReason: reason })
    .where(and(...filters))
}

export async function revokeUserSessions(
  userId: string,
  reason: RevocationReason = 'logout',
  now = Date.now(),
) {
  await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now), revokedReason: reason })
    .where(and(eq(schema.sessions.userId, userId), isNull(schema.sessions.revokedAt)))
}

function randomToken() {
  return randomBytes(32).toString('base64url')
}
