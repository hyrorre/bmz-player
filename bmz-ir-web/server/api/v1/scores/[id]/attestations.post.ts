import { createError, getRouterParam, readBody } from 'h3'
import {
  IrEvidenceValidationError,
  IrScoreNotFoundError,
  attestScore,
  validateScoreAttestation,
} from '../../../../services/ir'
import { requireIrUser } from '../../../../utils/auth'
import { SCORE_SUBMIT_RATE_LIMIT, checkUserRateLimit } from '../../../../utils/rate_limit'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  await checkUserRateLimit(event, 'score_submit', user.id, SCORE_SUBMIT_RATE_LIMIT)
  const scoreId = getRouterParam(event, 'id')
  if (!scoreId) {
    throw createError({ statusCode: 400, statusMessage: 'score id is required' })
  }
  try {
    const payload = validateScoreAttestation(await readBody(event))
    return await attestScore(user, scoreId, payload)
  } catch (error) {
    if (error instanceof IrScoreNotFoundError) {
      throw createError({ statusCode: 404, statusMessage: error.message })
    }
    if (error instanceof IrEvidenceValidationError) {
      throw createError({ statusCode: 400, statusMessage: error.message })
    }
    throw error
  }
})
