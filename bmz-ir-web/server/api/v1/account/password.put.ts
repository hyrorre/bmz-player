import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'
import { readPassword, requirePassword } from '../../../utils/auth_input'
import { revokeUserSessions } from '../../../utils/auth_tokens'

interface PasswordBody {
  current_password?: string
  password?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as PasswordBody
  const currentPassword = readPassword(body.current_password)
  const password = requirePassword(body.password)

  if (!currentPassword) {
    throw createError({ statusCode: 400, statusMessage: 'current_password is required' })
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
