import { getQuery } from 'h3'
import {
  ModerationAdminError,
  parseChartModerationStatus,
  parsePage,
  parseSearch,
  searchCharts,
} from '../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../../utils/admin'

const SEARCH_FIELDS = new Set(['any', 'sha256', 'md5', 'title', 'artist'])

/** 譜面検索。SHA-256 / MD5 は完全一致、title / artist は部分一致。 */
export default defineEventHandler(async (event) => {
  await requireIrAdmin(event)
  const query = getQuery(event)
  return handleModerationAdmin(() => {
    const field = typeof query.field === 'string' && query.field ? query.field : 'any'
    if (!SEARCH_FIELDS.has(field)) {
      throw new ModerationAdminError(400, 'field is invalid')
    }
    return searchCharts({
      search: parseSearch(query.q),
      field: field as 'any' | 'sha256' | 'md5' | 'title' | 'artist',
      status: query.status ? parseChartModerationStatus(query.status) : undefined,
      ...parsePage(query),
    })
  })
})
