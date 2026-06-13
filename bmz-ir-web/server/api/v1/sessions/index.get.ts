import { requireIrUser } from '../../../utils/auth'
import { listUserSessions } from '../../../utils/auth_tokens'

export default defineEventHandler(async (event) => {
  const user = await requireIrUser(event)
  return { sessions: await listUserSessions(user.id) }
})
