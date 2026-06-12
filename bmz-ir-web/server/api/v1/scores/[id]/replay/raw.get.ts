import { eq } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'

export default defineEventHandler(async (event) => {
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }

  const replay = await db.query.replayObjects.findFirst({
    columns: { objectPath: true, status: true, format: true },
    where: eq(schema.replayObjects.scoreId, scoreId),
  })
  if (!replay?.objectPath || !['uploaded', 'verified'].includes(replay.status)) {
    throw createError({ statusCode: 404, statusMessage: 'Replay is not available' })
  }

  const stored = await blob.get(replay.objectPath)
  if (!stored) {
    throw createError({ statusCode: 404, statusMessage: 'Replay object is not available' })
  }

  return new Response(stored, {
    headers: {
      'content-type': 'application/octet-stream',
      'x-bmz-replay-format': replay.format,
    },
  })
})
