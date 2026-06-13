import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'
import { normalizeEmail, readPassword } from '../../../utils/auth_input'

interface EmailBody {
  current_password?: string
  email?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as EmailBody
  const currentPassword = readPassword(body.current_password)
  const email = normalizeEmail(body.email)

  if (!currentPassword) {
    throw createError({ statusCode: 400, statusMessage: 'current_password is required' })
  }
  if (!email) {
    throw createError({ statusCode: 400, statusMessage: 'email is required' })
  }

  const account = await db.query.users.findFirst({
    columns: { passwordHash: true },
    where: eq(schema.users.id, user.id),
  })
  if (!account || !(await verifyPassword(account.passwordHash, currentPassword))) {
    throw createError({ statusCode: 401, statusMessage: 'Invalid current password' })
  }

  const existing = await db.query.users.findFirst({
    columns: { id: true },
    where: eq(schema.users.email, email),
  })
  if (existing && existing.id !== user.id) {
    throw createError({ statusCode: 409, statusMessage: 'email is already registered' })
  }

  await db
    .update(schema.users)
    .set({ email, updatedAt: new Date() })
    .where(eq(schema.users.id, user.id))
  const profile = await db.query.profiles.findFirst({
    columns: { displayName: true },
    where: eq(schema.profiles.id, user.id),
  })
  const displayName = profile?.displayName ?? ''
  await replaceUserSession(event, {
    user: { id: user.id, email, displayName },
  })

  return { player: { id: user.id, email, display_name: displayName } }
})
