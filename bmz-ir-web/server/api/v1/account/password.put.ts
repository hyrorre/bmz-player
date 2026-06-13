import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'
import { revokeUserSessions } from '../../../utils/auth_tokens'

interface PasswordBody {
  current_password?: string
  password?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as PasswordBody
  const currentPassword = body.current_password ?? ''
  const password = body.password ?? ''

  if (!currentPassword) {
    throw createError({ statusCode: 400, statusMessage: 'current_password is required' })
  }
  if (password.length < 8) {
    throw createError({ statusCode: 400, statusMessage: 'password is too short' })
  }

  const account = await db.query.users.findFirst({
    columns: { passwordHash: true },
    where: eq(schema.users.id, user.id),
  })
  if (!account || !(await verifyPassword(account.passwordHash, currentPassword))) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid current password' })
  }

  await db
    .update(schema.users)
    .set({ passwordHash: await hashPassword(password), updatedAt: new Date() })
    .where(eq(schema.users.id, user.id))

  await revokeUserSessions(user.id, 'password_changed')
  await clearUserSession(event)

  return { updated: true, logged_out: true }
})
