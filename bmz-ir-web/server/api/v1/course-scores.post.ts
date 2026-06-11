import { serverSupabaseServiceRole } from '#supabase/server'
import { readBody } from 'h3'
import { submitCourseScore, validateCourseScoreSubmission } from '../../services/course_ir'
import { requireIrUser } from '../../utils/auth'
import type { Database } from '../../../shared/types/database.types'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  const payload = validateCourseScoreSubmission(await readBody(event))
  const db = serverSupabaseServiceRole<Database>(event)
  return submitCourseScore(db, user, payload)
})
