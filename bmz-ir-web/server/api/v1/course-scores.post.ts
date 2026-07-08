import { createError, readBody } from 'h3'
import { submitCourseScore, validateCourseScoreSubmission } from '../../services/course_ir'
import { requireIrUser } from '../../utils/auth'
import { SCORE_SUBMIT_RATE_LIMIT, checkUserRateLimit } from '../../utils/rate_limit'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  // 単曲スコアと同じ score_submit アクションで数える。
  await checkUserRateLimit(event, 'score_submit', user.id, SCORE_SUBMIT_RATE_LIMIT)
  let payload
  try {
    payload = validateCourseScoreSubmission(await readBody(event))
  } catch (error) {
    throw createError({
      statusCode: 400,
      statusMessage: error instanceof Error ? error.message : 'invalid course score payload',
    })
  }
  return submitCourseScore(user, payload)
})
