import { eq, or } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'
import { requireIrUser } from '../../../utils/auth'
import { readPassword } from '../../../utils/auth_input'

interface DeleteAccountBody {
  current_password?: string
}

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const body = (await readBody(event)) as DeleteAccountBody
  const currentPassword = readPassword(body.current_password)

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

  const replayObjects = await db.query.replayObjects.findMany({
    columns: { objectPath: true },
    where: eq(schema.replayObjects.playerId, user.id),
  })
  const objectPaths = replayObjects
    .map((replay) => replay.objectPath)
    .filter((path): path is string => !!path)

  for (const objectPath of objectPaths) {
    await blob.delete(objectPath)
  }

  await db.transaction(async (tx) => {
    await tx.delete(schema.bestCourseScores).where(eq(schema.bestCourseScores.playerId, user.id))
    await tx.delete(schema.bestScores).where(eq(schema.bestScores.playerId, user.id))
    await tx.delete(schema.replayObjects).where(eq(schema.replayObjects.playerId, user.id))
    await tx
      .delete(schema.rivalRelationships)
      .where(
        or(
          eq(schema.rivalRelationships.ownerPlayerId, user.id),
          eq(schema.rivalRelationships.targetPlayerId, user.id),
        ),
      )
    await tx.delete(schema.deviceKeys).where(eq(schema.deviceKeys.playerId, user.id))
    await tx.delete(schema.courseScores).where(eq(schema.courseScores.playerId, user.id))
    await tx.delete(schema.scores).where(eq(schema.scores.playerId, user.id))
    await tx.delete(schema.sessions).where(eq(schema.sessions.userId, user.id))
    await tx.delete(schema.profiles).where(eq(schema.profiles.id, user.id))
    await tx.delete(schema.users).where(eq(schema.users.id, user.id))
  })

  await clearUserSession(event)

  return { deleted: true }
})
