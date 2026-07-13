import { requireIrAdmin } from '../../../../utils/admin'

export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const task = await runTask('difficulty-tables:sync')
  return task.result
})
