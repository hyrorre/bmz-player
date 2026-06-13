import { randomUUID } from 'node:crypto'
import { createError, readBody } from 'h3'
import { db, schema } from 'hub:db'
import { normalizeDisplayName, normalizeEmail, requirePassword } from '../../../utils/auth_input'
import { checkAuthRateLimit } from '../../../utils/auth_rate_limit'
import { createAuthTokens } from '../../../utils/auth_tokens'

interface RegisterBody {
  client_type?: 'web' | 'desktop'
  email?: string
  password?: string
  display_name?: string
}

export default defineEventHandler(async (event) => {
  const body = (await readBody(event)) as RegisterBody
  const email = normalizeEmail(body.email)
  const displayName = normalizeDisplayName(body.display_name)

  if (!email || !displayName) {
    throw createError({
      statusCode: 400,
      statusMessage: 'email, password, and display_name are required',
    })
  }
  const password = requirePassword(body.password)
  await checkAuthRateLimit(event, 'register', email)

  const userId = randomUUID()
  try {
    await db.insert(schema.users).values({
      id: userId,
      email,
      passwordHash: await hashPassword(password),
    })
    await db.insert(schema.profiles).values({
      id: userId,
      displayName,
    })
  } catch (error) {
    throw createError({
      statusCode: 409,
      statusMessage: 'Account already exists',
      cause: error,
    })
  }

  const tokens = await createAuthTokens(userId, { clientType: body.client_type ?? 'web' })
  await setUserSession(event, {
    user: {
      id: userId,
      email,
      displayName,
    },
  })

  return {
    access_token: tokens.accessToken,
    refresh_token: tokens.refreshToken,
    expires_at: tokens.accessExpiresAt,
    player: {
      id: userId,
      email,
      display_name: displayName,
    },
  }
})
