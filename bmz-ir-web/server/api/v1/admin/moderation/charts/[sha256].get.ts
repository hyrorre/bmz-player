import {
  getChartModerationDetail,
  parseChartSha256,
} from '../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../../utils/admin'

export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  return handleModerationAdmin(() =>
    getChartModerationDetail(parseChartSha256(getRouterParam(event, 'sha256'))),
  )
})
