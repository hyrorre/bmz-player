import { and, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { id: true, objectPath: true },
    where: and(
      eq(schema.replayObjects.scoreId, scoreId),
      eq(schema.replayObjects.status, 'pending_upload'),
    ),
  })
  if (!replay?.objectPath) {
    throw createError({ statusCode: 404, statusMessage: 'Replay upload intent not found' })
  }

  const body = await readRawBody(event, false)
  if (!body || body.length === 0) {
    throw createError({ statusCode: 400, statusMessage: 'Replay body is required' })
  }

  await useStorage('replays').setItemRaw(replay.objectPath, body)
  await db
    .update(schema.replayObjects)
    .set({ status: 'uploaded', sizeBytes: body.length, updatedAt: new Date() })
    .where(eq(schema.replayObjects.id, replay.id))

  return { uploaded: true, size_bytes: body.length }
})
