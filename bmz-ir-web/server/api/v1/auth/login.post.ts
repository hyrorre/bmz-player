import { readBody, createError } from 'h3'
import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { normalizeEmail, readPassword } from '../../../utils/auth_input'
import { checkAuthRateLimit } from '../../../utils/auth_rate_limit'
import {
  getUserModerationStatus,
  moderationHttpError,
} from '../../../services/moderation/enforcement'
import { createAuthTokens } from '../../../utils/auth_tokens'
import { irProviderKeyForEvent } from '../../../utils/provider_key'

interface LoginBody {
  client_type?: 'web' | 'desktop'
  email?: string
  password?: string
}

export default defineEventHandler(async (event) => {
  const body = (await readBody(event)) as LoginBody
  const email = normalizeEmail(body?.email)
  const password = readPassword(body?.password)
  if (!email || !password) {
    throw createError({ statusCode: 400, statusMessage: 'email and password are required' })
  }
  await checkAuthRateLimit(event, 'login', email)

  const rows = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      passwordHash: schema.users.passwordHash,
      displayName: schema.profiles.displayName,
    })
    .from(schema.users)
    .leftJoin(schema.profiles, eq(schema.profiles.id, schema.users.id))
    .where(eq(schema.users.email, email))
    .limit(1)
  const user = rows[0]
  if (!user || !(await verifyPassword(user.passwordHash, password))) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid credentials' })
  }
  // パスワード確認後にだけ状態を返し、BANの有無をメールアドレスから推測させない。
  if ((await getUserModerationStatus(user.id)) === 'banned') {
    throw moderationHttpError('ACCOUNT_BANNED')
  }

  const tokens = await createAuthTokens(user.id, { clientType: body.client_type ?? 'web' })
  await setUserSession(event, {
    secure: { sessionGroupId: tokens.sessionGroupId },
    user: {
      id: user.id,
      email: user.email,
      displayName: user.displayName ?? '',
    },
  })

  return {
    provider_key: irProviderKeyForEvent(event),
    access_token: tokens.accessToken,
    refresh_token: tokens.refreshToken,
    expires_at: tokens.accessExpiresAt,
    player: {
      id: user.id,
      email: user.email,
      display_name: user.displayName ?? null,
    },
  }
})
