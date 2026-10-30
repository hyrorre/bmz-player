import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { and, eq, gt, inArray, isNull, or, sql, type SQL } from 'drizzle-orm'
import { alias } from 'drizzle-orm/sqlite-core'
import { db, schema } from 'hub:db'
import type { UserModerationStatus } from '../../shared/constants/moderation'
import { moderationHttpError } from '../services/moderation/enforcement'
import { publicPlayerCondition } from '../services/moderation/visibility'
import { revokedRefreshTokenAction, type SessionRevocationReason } from './auth_token_policy'

const ACCESS_TOKEN_TTL_SECONDS = 60 * 60
const REFRESH_TOKEN_TTL_SECONDS = 60 * 60 * 24 * 30
type SessionKind = 'access' | 'refresh'
type ClientType = 'web' | 'desktop'
type RevocationReason = SessionRevocationReason
/** 利用者・管理者による明示的な失効。rotation と再利用検知以外。 */
type ExplicitRevocationReason = Exclude<RevocationReason, 'rotated' | 'reuse_detected'>

export interface AuthTokenPair {
  sessionGroupId: string
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

/**
 * ログイン・登録で新しいセッションを作る。ユーザーがBANされていないことを条件にした1文で
 * access / refresh token の組を追加するため、BAN確認の後にBANが確定しても有効なtokenは残らない
 * （追加の後にBANが確定した場合は、BANがこのtokenを失効させる）。追加されなかった場合は
 * ACCOUNT_BANNED を投げ、tokenを返さない。
 */
export async function createAuthTokens(
  userId: string,
  options: CreateAuthTokensOptions = {},
): Promise<AuthTokenPair> {
  const now = options.now ?? Date.now()
  const tokens = await insertTokenPair(
    {
      userId,
      clientType: options.clientType ?? 'web',
      sessionGroupId: options.sessionGroupId ?? randomUUID(),
    },
    now,
    publicPlayerCondition(sql`${userId}`),
  )
  if (!tokens) {
    throw moderationHttpError('ACCOUNT_BANNED')
  }
  return tokens
}

/**
 * access / refresh token の組を、`condition` が成立する場合だけ1文の INSERT で追加する。
 * 片方だけ保存されることはなく、追加されなかった場合と DB エラーの場合（例外）は token を返さない。
 */
async function insertTokenPair(
  session: { userId: string; clientType: ClientType; sessionGroupId: string },
  now: number,
  condition: SQL,
): Promise<AuthTokenPair | null> {
  const sessions = schema.sessions
  const accessToken = randomToken()
  const refreshToken = randomToken()
  const accessExpiresAt = Math.floor(now / 1000) + ACCESS_TOKEN_TTL_SECONDS
  const refreshExpiresAt = now + REFRESH_TOKEN_TTL_SECONDS * 1000
  const row = (tokenHash: string, kind: SessionKind, expiresAt: number) =>
    sql`select ${tokenHash}, ${session.sessionGroupId}, ${session.userId}, ${kind},
      ${session.clientType}, ${expiresAt}, null, null, null, ${now}
    where ${condition}`
  // insert().select() の SELECT の列順は sessions の列定義順。
  const inserted = await db
    .insert(sessions)
    .select(
      sql`${row(hashToken(accessToken), 'access', accessExpiresAt * 1000)}
        union all ${row(hashToken(refreshToken), 'refresh', refreshExpiresAt)}`,
    )
    .returning({ tokenHash: sessions.tokenHash })
  if (inserted.length !== 2) {
    return null
  }
  return { sessionGroupId: session.sessionGroupId, accessToken, refreshToken, accessExpiresAt }
}

export async function findUserByWebSession(
  userId: string,
  sessionGroupId: string,
  now = Date.now(),
) {
  const rows = await db
    .select({ id: schema.users.id, email: schema.users.email })
    .from(schema.sessions)
    .innerJoin(schema.users, eq(schema.sessions.userId, schema.users.id))
    .where(
      and(
        eq(schema.sessions.userId, userId),
        eq(schema.sessions.sessionGroupId, sessionGroupId),
        eq(schema.sessions.clientType, 'web'),
        eq(schema.sessions.kind, 'refresh'),
        isNull(schema.sessions.revokedAt),
        gt(schema.sessions.expiresAt, new Date(now)),
        // BAN時のセッション失効だけに頼らず、毎回現在の状態を確認する。
        publicPlayerCondition(schema.users.id),
      ),
    )
    .limit(1)
  return rows[0] ?? null
}

export async function revokeWebSession(userId: string, sessionGroupId: string) {
  const [retire, revoke] = revokeSessionsStatements(
    and(
      eq(schema.sessions.userId, userId),
      eq(schema.sessions.sessionGroupId, sessionGroupId),
      eq(schema.sessions.clientType, 'web'),
    ),
    'logout',
    new Date(),
  )
  await db.batch([retire, revoke])
}

/**
 * access token からユーザーを引く。BAN済みユーザーは `moderationStatus: 'banned'` で返し、
 * 呼び出し側で識別可能なエラーにする。BAN時にtokenは失効させるが、失効と同時に
 * 発行されたtokenが残る競合でも、ここで現在の状態を確認する。
 */
export async function findUserByAccessToken(token: string, now = Date.now()) {
  const rows = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      displayName: schema.profiles.displayName,
      moderationStatus: schema.userModeration.status,
    })
    .from(schema.sessions)
    .innerJoin(schema.users, eq(schema.sessions.userId, schema.users.id))
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .leftJoin(schema.userModeration, eq(schema.userModeration.userId, schema.users.id))
    .where(
      and(
        eq(schema.sessions.tokenHash, hashToken(token)),
        eq(schema.sessions.kind, 'access'),
        isNull(schema.sessions.revokedAt),
        gt(schema.sessions.expiresAt, new Date(now)),
      ),
    )
    .limit(1)

  const row = rows[0] ?? null
  if (!row) {
    return null
  }
  const user = {
    ...row,
    moderationStatus: (row.moderationStatus ?? 'active') as UserModerationStatus,
  }
  if (user.moderationStatus !== 'banned') {
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
      revokedReason: true,
    },
    where: and(
      eq(schema.sessions.tokenHash, hashToken(token)),
      eq(schema.sessions.kind, 'refresh'),
    ),
  })

  if (!session) {
    return null
  }

  // tokenの所持を確認した後、失効状態より先にBANを判定して識別可能な応答にする。
  // BAN済みならtokenを新しく発行せず、残っているセッションもすべて失効させる。
  const moderation = await db.query.userModeration.findFirst({
    columns: { status: true },
    where: eq(schema.userModeration.userId, session.userId),
  })
  if (moderation?.status === 'banned') {
    return await refuseBannedRefresh(session.userId, now)
  }

  if (session.revokedAt) {
    const action = revokedRefreshTokenAction(session.revokedAt, session.revokedReason, now)
    if (action !== 'reuse_detected') {
      return null
    }
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

  // ここまでの読み取りと以降の書き込みの間に、BAN・ログアウト・同じtokenでの並行refreshが
  // 割り込み得る。各書き込みは、その時点の状態を条件に含む1文として実行する（D1は書き込みを
  // 1文ずつ直列に実行し、1文は原子的に適用される）。
  //
  // 1. 旧tokenが有効で、ユーザーがBANされていない場合だけ rotated にする。BANなどで失効済みの
  //    tokenを rotated で上書きせず、並行refreshでは片方だけが成功する。
  const rotated = await db
    .update(schema.sessions)
    .set({ revokedAt: new Date(now), revokedReason: 'rotated' })
    .where(
      and(
        eq(schema.sessions.tokenHash, session.tokenHash),
        isNull(schema.sessions.revokedAt),
        publicPlayerCondition(schema.sessions.userId),
      ),
    )
    .returning({ tokenHash: schema.sessions.tokenHash })
  if (rotated.length === 0) {
    return await refreshLostRace(session.userId, now)
  }

  // 2. 新しいtokenの組を1文で追加する。1と2の間にBANが確定すると、BANは旧tokenを admin に
  //    付け替えるため、この条件で何も追加されない。2の後にBANが確定した場合は、BANが新しい
  //    tokenを失効させる。どちらの順でも、BAN後に有効なtokenは残らない。
  const tokens = await insertRotatedTokens(session, now)
  if (!tokens) {
    return await refreshLostRace(session.userId, now)
  }

  return {
    tokens,
    user: {
      id: session.userId,
      email: user.email,
      displayName: user.displayName ?? '',
    },
  }
}

/**
 * BAN済みユーザーのrefreshを拒否し、残っているセッションを打ち切る。失効は書き込み時点でも
 * BAN中の場合だけ行うため、BANを読み取った後に解除・再ログインされても、新しいセッションは
 * 失効させない（その場合も、このrequestにはBANを読み取った時点の結果として403を返す）。
 */
async function refuseBannedRefresh(userId: string, now: number) {
  const [revokeActive, retireRevoked] = banSessionStatements(userId, new Date(now))
  await db.batch([revokeActive, retireRevoked])
  return { banned: true as const }
}

/**
 * rotation の書き込みが競合で成立しなかった。BANが割り込んだなら ACCOUNT_BANNED、
 * それ以外（並行refresh・ログアウトなど）は、tokenを発行せず拒否だけする。
 */
async function refreshLostRace(userId: string, now: number) {
  const moderation = await db.query.userModeration.findFirst({
    columns: { status: true },
    where: eq(schema.userModeration.userId, userId),
  })
  return moderation?.status === 'banned' ? await refuseBannedRefresh(userId, now) : null
}

/**
 * rotation 後の access / refresh token の組を、旧tokenがこの rotation の結果のまま残っていて、
 * ユーザーがBANされていない場合だけ追加する。rotation の後に BAN・ログアウト・パスワード変更・
 * 個別失効が確定すると、旧tokenはその失効理由に付け替わるため、何も追加されない。
 */
function insertRotatedTokens(
  session: {
    tokenHash: string
    sessionGroupId: string | null
    userId: string
    clientType: ClientType
  },
  now: number,
) {
  const sessions = schema.sessions
  const stillRotated = sql`exists (select 1 from ${sessions} as rotated_session
    where rotated_session.token_hash = ${session.tokenHash}
      and rotated_session.revoked_reason = 'rotated'
      and rotated_session.revoked_at = ${now})`
  return insertTokenPair(
    {
      userId: session.userId,
      clientType: session.clientType,
      sessionGroupId: session.sessionGroupId ?? randomUUID(),
    },
    now,
    sql`${stillRotated} and ${publicPlayerCondition(sql`${session.userId}`)}`,
  )
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
  reason: ExplicitRevocationReason = 'logout',
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

  const [retire, revoke] = revokeSessionsStatements(
    and(
      eq(schema.sessions.userId, userId),
      or(eq(schema.sessions.sessionGroupId, groupKey), eq(schema.sessions.tokenHash, groupKey)),
    ),
    reason,
    new Date(now),
  )
  await db.batch([retire, revoke])
  return true
}

/**
 * 提示された token が有効（失効しておらず期限内）な場合だけ、その token が属するセッション
 * グループ全体を失効させる（ログアウト）。グループはサーバー側で token から求め、request で
 * user ID やグループを指定させない。失効済み・期限切れの token を提示しても何も変更しないため、
 * 盗まれた古い token で他のセッションを操作することはできない。
 *
 * グループ単位で失効させるのは、同じグループで並行している refresh を残さないため。refresh が
 * 先に確定していれば新しい token もこのグループに入って失効し、rotation と新しい token の追加の
 * 間にログアウトが確定すれば、旧 token が付け替わって追加が成立しない。
 */
export async function revokeToken(
  token: string,
  kind?: SessionKind,
  reason: ExplicitRevocationReason = 'logout',
  now = Date.now(),
) {
  const sessions = schema.sessions
  const presented = alias(sessions, 'presented_session')
  const presentedFilter = and(
    eq(presented.tokenHash, hashToken(token)),
    kind ? eq(presented.kind, kind) : undefined,
    isNull(presented.revokedAt),
    gt(presented.expiresAt, new Date(now)),
  )
  const presentedGroup = db
    .select({ sessionGroupId: presented.sessionGroupId })
    .from(presented)
    .where(presentedFilter)
  const presentedToken = db
    .select({ tokenHash: presented.tokenHash })
    .from(presented)
    .where(presentedFilter)
  // session_group_id が無い古い行は、その token だけを対象にする。
  const scope = or(
    inArray(sessions.sessionGroupId, presentedGroup),
    inArray(sessions.tokenHash, presentedToken),
  )
  const [retire, revoke] = revokeSessionsStatements(scope, reason, new Date(now))
  await db.batch([retire, revoke])
}

/**
 * BAN でユーザーのセッションを打ち切る更新。呼び出し側が同じ batch で実行する。
 *
 * 1. 有効なセッションを `admin` で失効させる。
 * 2. BAN より前に rotation・再利用検知で失効していた token も `admin` に付け替える。
 *    これらは失効済みのまま残り、解除後に古い端末が提示すると、再利用検知として解除後の
 *    新しいセッションまで失効させてしまう。`admin` にしておけば拒否するだけになる。
 *    付け替えは BAN を境に単調に進み、時刻の比較に頼らない。解除後に発行・rotation された
 *    token には触れないため、解除後の再利用検知はそのまま働く。
 *
 * どちらも書き込み時点でユーザーがBAN中の場合だけ行う。
 * `condition` は管理操作が実際に適用された場合だけ実行するための条件。
 */
export function banSessionStatements(userId: string, now: Date, condition?: SQL) {
  // 書き込み時点でBAN中であることを条件にする。BANを読み取った後に解除された場合、解除後の
  // ログインで作られたセッションを失効させない。
  const stillBanned = sql`not ${publicPlayerCondition(sql`${userId}`)}`
  const [retireRevoked, revokeActive] = revokeSessionsStatements(
    and(eq(schema.sessions.userId, userId), stillBanned, condition),
    'admin',
    now,
  )
  return [revokeActive, retireRevoked] as const
}

/** 明示的な失効操作で付け替える対象。再利用検知の根拠になる失効理由。 */
const RETIRABLE_REASONS = ['rotated', 'reuse_detected'] satisfies RevocationReason[]

/**
 * ログアウト・パスワード変更・個別失効など、利用者が範囲を指定してセッションを打ち切る更新。
 * 呼び出し側が同じ batch で実行する。
 *
 * 1. 範囲内で、過去に rotation・再利用検知で失効した token を今回の失効理由に付け替える。
 *    付け替えないと、打ち切った後に古い token が提示されたとき再利用検知になり、その後の
 *    ログインで作られた正常なセッションまで失効させてしまう。範囲内の token はこの操作で
 *    すべて無効になるため、そこから再利用を検知して守るべきセッションは残っていない。
 * 2. 範囲内の有効な token を失効させる。
 *
 * 範囲外（別のセッショングループ）の token には触れないため、そこでの再利用検知はそのまま働く。
 * 再利用検知そのもの（`reuse_detected`）では付け替えない。
 */
export function revokeSessionsStatements(
  scope: SQL | undefined,
  reason: ExplicitRevocationReason,
  now: Date,
) {
  const sessions = schema.sessions
  return [
    db
      .update(sessions)
      .set({ revokedReason: reason })
      .where(and(scope, inArray(sessions.revokedReason, RETIRABLE_REASONS))),
    db
      .update(sessions)
      .set({ revokedAt: now, revokedReason: reason })
      .where(and(scope, isNull(sessions.revokedAt))),
  ] as const
}

export async function revokeUserSessions(
  userId: string,
  reason: RevocationReason = 'logout',
  now = Date.now(),
) {
  const scope = eq(schema.sessions.userId, userId)
  if (reason === 'rotated' || reason === 'reuse_detected') {
    await db
      .update(schema.sessions)
      .set({ revokedAt: new Date(now), revokedReason: reason })
      .where(and(scope, isNull(schema.sessions.revokedAt)))
    return
  }
  const [retire, revoke] = revokeSessionsStatements(scope, reason, new Date(now))
  await db.batch([retire, revoke])
}

function randomToken() {
  return randomBytes(32).toString('base64url')
}

function publicSessionId(userId: string, groupKey: string) {
  return createHash('sha256').update(`${userId}:${groupKey}`).digest('base64url')
}
