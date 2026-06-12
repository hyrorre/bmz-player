import { createHash, randomBytes } from 'node:crypto'
import { and, eq, gt, isNull } from 'drizzle-orm'
import { db, schema } from 'hub:db'

const ACCESS_TOKEN_TTL_SECONDS = 60 * 60
const REFRESH_TOKEN_TTL_SECONDS = 60 * 60 * 24 * 30

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
  const rows = await db
    .select({
      tokenHash: schema.sessions.tokenHash,
      userId: schema.sessions.userId,
      email: schema.users.email,
      displayName: schema.profiles.displayName,
    })
    .from(schema.sessions)
    .innerJoin(schema.users, eq(schema.sessions.userId, schema.users.id))
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .where(
      and(
        eq(schema.sessions.tokenHash, hashToken(token)),
        eq(schema.sessions.kind, 'refresh'),
        isNull(schema.sessions.revokedAt),
        gt(schema.sessions.expiresAt, new Date(now)),
      ),
    )
    .limit(1)

  const session = rows[0]
  if (!session) {
    return null
  }

  await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now) })
    .where(eq(schema.sessions.tokenHash, session.tokenHash))

  return {
    tokens: await createAuthTokens(session.userId, now),
    user: {
      id: session.userId,
      email: session.email,
      displayName: session.displayName ?? '',
    },
  }
}

function randomToken() {
  return randomBytes(32).toString('base64url')
}
