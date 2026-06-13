import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { and, eq, gt, isNull, or } from 'drizzle-orm'
import { db, schema } from 'hub:db'

const ACCESS_TOKEN_TTL_SECONDS = 60 * 60
const REFRESH_TOKEN_TTL_SECONDS = 60 * 60 * 24 * 30
type SessionKind = 'access' | 'refresh'
type ClientType = 'web' | 'desktop'
type RevocationReason = 'logout' | 'rotated' | 'password_changed' | 'reuse_detected' | 'admin'

export interface AuthTokenPair {
  accessToken: string
  refreshToken: string
  accessExpiresAt: number
}

export interface UserSessionSummary {
  id: string
  clientType: ClientType
  createdAt: string
  expiresAt: string
  lastUsedAt: string | null
  hasAccessToken: boolean
  hasRefreshToken: boolean
}

interface CreateAuthTokensOptions {
  clientType?: ClientType
  now?: number
  sessionGroupId?: string
}

export function hashToken(token: string) {
  return createHash('sha256').update(token).digest('hex')
}

export async function createAuthTokens(
  userId: string,
  options: CreateAuthTokensOptions = {},
): Promise<AuthTokenPair> {
  const now = options.now ?? Date.now()
  const clientType = options.clientType ?? 'web'
  const sessionGroupId = options.sessionGroupId ?? randomUUID()
  const accessToken = randomToken()
  const refreshToken = randomToken()
  const accessExpiresAt = Math.floor(now / 1000) + ACCESS_TOKEN_TTL_SECONDS
  const refreshExpiresAt = now + REFRESH_TOKEN_TTL_SECONDS * 1000

  await db.insert(schema.sessions).values([
    {
      tokenHash: hashToken(accessToken),
      sessionGroupId,
      userId,
      kind: 'access',
      clientType,
      expiresAt: new Date(accessExpiresAt * 1000),
    },
    {
      tokenHash: hashToken(refreshToken),
      sessionGroupId,
      userId,
      kind: 'refresh',
      clientType,
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

  const user = rows[0] ?? null
  if (user) {
    await db
      .update(schema.sessions)
      .set({ lastUsedAt: new Date(now) })
      .where(eq(schema.sessions.tokenHash, hashToken(token)))
  }

  return user
}

export async function rotateRefreshToken(token: string, now = Date.now()) {
  const session = await db.query.sessions.findFirst({
    columns: {
      tokenHash: true,
      sessionGroupId: true,
      userId: true,
      clientType: true,
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
    tokens: await createAuthTokens(session.userId, {
      clientType: session.clientType,
      now,
      sessionGroupId: session.sessionGroupId ?? randomUUID(),
    }),
    user: {
      id: session.userId,
      email: user.email,
      displayName: user.displayName ?? '',
    },
  }
}

export async function listUserSessions(
  userId: string,
  now = Date.now(),
): Promise<UserSessionSummary[]> {
  const rows = await db.query.sessions.findMany({
    columns: {
      tokenHash: true,
      sessionGroupId: true,
      kind: true,
      clientType: true,
      expiresAt: true,
      lastUsedAt: true,
      createdAt: true,
    },
    where: and(
      eq(schema.sessions.userId, userId),
      isNull(schema.sessions.revokedAt),
      gt(schema.sessions.expiresAt, new Date(now)),
    ),
  })

  const groups = new Map<
    string,
    {
      clientType: ClientType
      createdAt: Date
      expiresAt: Date
      lastUsedAt: Date | null
      hasAccessToken: boolean
      hasRefreshToken: boolean
    }
  >()

  for (const row of rows) {
    const groupKey = row.sessionGroupId ?? row.tokenHash
    const existing = groups.get(groupKey)
    if (!existing) {
      groups.set(groupKey, {
        clientType: row.clientType,
        createdAt: row.createdAt,
        expiresAt: row.expiresAt,
        lastUsedAt: row.lastUsedAt,
        hasAccessToken: row.kind === 'access',
        hasRefreshToken: row.kind === 'refresh',
      })
      continue
    }

    if (row.createdAt < existing.createdAt) {
      existing.createdAt = row.createdAt
    }
    if (row.expiresAt > existing.expiresAt) {
      existing.expiresAt = row.expiresAt
    }
    if (row.lastUsedAt && (!existing.lastUsedAt || row.lastUsedAt > existing.lastUsedAt)) {
      existing.lastUsedAt = row.lastUsedAt
    }
    existing.hasAccessToken ||= row.kind === 'access'
    existing.hasRefreshToken ||= row.kind === 'refresh'
  }

  return [...groups.entries()]
    .map(([groupKey, session]) => ({
      id: publicSessionId(userId, groupKey),
      clientType: session.clientType,
      createdAt: session.createdAt.toISOString(),
      expiresAt: session.expiresAt.toISOString(),
      lastUsedAt: session.lastUsedAt?.toISOString() ?? null,
      hasAccessToken: session.hasAccessToken,
      hasRefreshToken: session.hasRefreshToken,
    }))
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
}

export async function revokeUserSessionById(
  userId: string,
  sessionId: string,
  reason: RevocationReason = 'logout',
  now = Date.now(),
) {
  const rows = await db.query.sessions.findMany({
    columns: { tokenHash: true, sessionGroupId: true },
    where: and(
      eq(schema.sessions.userId, userId),
      isNull(schema.sessions.revokedAt),
      gt(schema.sessions.expiresAt, new Date(now)),
    ),
  })
  const groupKey = rows
    .map((row) => row.sessionGroupId ?? row.tokenHash)
    .find((groupKey) => publicSessionId(userId, groupKey) === sessionId)
  if (!groupKey) {
    return false
  }

  await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now), revokedReason: reason })
    .where(
      and(
        eq(schema.sessions.userId, userId),
        isNull(schema.sessions.revokedAt),
        or(eq(schema.sessions.sessionGroupId, groupKey), eq(schema.sessions.tokenHash, groupKey)),
      ),
    )
  return true
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

function publicSessionId(userId: string, groupKey: string) {
  return createHash('sha256').update(`${userId}:${groupKey}`).digest('base64url')
}
