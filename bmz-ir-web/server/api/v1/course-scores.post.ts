import { createError, readBody } from 'h3'
import { submitCourseScore, validateCourseScoreSubmission } from '../../services/course_ir'
import { requireIrUser } from '../../utils/auth'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
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
