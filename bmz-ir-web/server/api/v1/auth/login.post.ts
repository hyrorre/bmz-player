import { readBody, createError } from 'h3'
import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { normalizeEmail, readPassword } from '../../../utils/auth_input'
import { checkAuthRateLimit } from '../../../utils/auth_rate_limit'
import { createAuthTokens } from '../../../utils/auth_tokens'

interface LoginBody {
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

  const tokens = await createAuthTokens(user.id)
  await setUserSession(event, {
    user: {
      id: user.id,
      email: user.email,
      displayName: user.displayName ?? '',
    },
  })

  return {
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
