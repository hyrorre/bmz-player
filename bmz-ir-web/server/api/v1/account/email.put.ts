import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'

interface EmailBody {
  email?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as EmailBody
  const email = body.email?.trim().toLowerCase() ?? ''

  if (!email) {
    throw createError({ statusCode: 400, statusMessage: 'email is required' })
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
