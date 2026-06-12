import { randomUUID } from 'node:crypto'
import { createError, readBody } from 'h3'
import { db, schema } from 'hub:db'
import { createAuthTokens } from '../../../utils/auth_tokens'

interface SignupBody {
  email?: string
  password?: string
  display_name?: string
}

export default defineEventHandler(async (event) => {
  const body = (await readBody(event)) as SignupBody
  const email = body.email?.trim().toLowerCase()
  const password = body.password ?? ''
  const displayName = body.display_name?.trim() ?? ''

  if (!email || !password || !displayName) {
    throw createError({
      statusCode: 400,
      statusMessage: 'email, password, and display_name are required',
    })
  }
  if (password.length < 8) {
    throw createError({ statusCode: 400, statusMessage: 'password must be at least 8 characters' })
  }

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

  const tokens = await createAuthTokens(userId)
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
