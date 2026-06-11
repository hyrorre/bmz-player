import { serverSupabaseServiceRole } from '#supabase/server'
import { createError, readBody } from 'h3'
import { submitCourseScore, validateCourseScoreSubmission } from '../../services/course_ir'
import { requireIrUser } from '../../utils/auth'
import type { Database } from '../../../shared/types/database.types'

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
  const db = serverSupabaseServiceRole<Database>(event)
  return submitCourseScore(db, user, payload)
})
