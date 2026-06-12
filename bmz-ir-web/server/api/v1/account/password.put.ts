import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'

interface PasswordBody {
  password?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as PasswordBody
  const password = body.password ?? ''

  if (password.length < 8) {
    throw createError({ statusCode: 400, statusMessage: 'password is too short' })
  }

  await db
    .update(schema.users)
    .set({ passwordHash: await hashPassword(password), updatedAt: new Date() })
    .where(eq(schema.users.id, user.id))

  return { updated: true }
})
