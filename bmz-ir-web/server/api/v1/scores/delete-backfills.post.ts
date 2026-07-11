import { createError, readBody } from 'h3'
import {
  IrBackfillCleanupError,
  MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE,
  deleteLocalBackfillScores,
} from '../../../services/ir'
import { requireIrUser } from '../../../utils/auth'
import { SCORE_CLEANUP_RATE_LIMIT, checkUserRateLimit } from '../../../utils/rate_limit'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'score_cleanup', user.id, SCORE_CLEANUP_RATE_LIMIT)
  const body = await readBody<{ score_ids?: unknown }>(event)
  if (!Array.isArray(body?.score_ids)) {
    throw createError({ statusCode: 400, statusMessage: 'score_ids must be an array' })
  }
  if (!body.score_ids.every((id): id is string => typeof id === 'string')) {
    throw createError({ statusCode: 400, statusMessage: 'score_ids must contain unique score ids' })
  }
  const scoreIds = body.score_ids.map((id) => id.trim())
  if (scoreIds.some((id) => !id) || new Set(scoreIds).size !== scoreIds.length) {
    throw createError({ statusCode: 400, statusMessage: 'score_ids must contain unique score ids' })
  }
  if (scoreIds.length > MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE) {
    throw createError({
      statusCode: 400,
      statusMessage: `score_ids must contain at most ${MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE} entries`,
    })
  }
  try {
    return await deleteLocalBackfillScores(user, scoreIds)
  } catch (error) {
    if (error instanceof IrBackfillCleanupError) {
      throw createError({ statusCode: 409, statusMessage: error.message })
    }
    throw error
  }
})
