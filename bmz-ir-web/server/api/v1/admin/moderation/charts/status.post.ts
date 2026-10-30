import { readBody } from 'h3'
import {
  changeChartModeration,
  parseChartModerationItems,
  parseChartModerationStatus,
  parseNote,
  parseReason,
} from '../../../../../services/moderation/admin'
import { handleModerationAdmin } from '../../../../../services/moderation/http'
import { requireIrAdmin } from '../../../../../utils/admin'

interface ChartStatusBody {
  items?: unknown
  status?: unknown
  reason?: unknown
  note?: unknown
}

/**
 * 指定した SHA-256 の譜面を1件以上まとめて状態変更する (最大 50 件)。
 *
 * 応答の results は譜面ごとに changed / unchanged / conflict を返す。conflict の譜面は
 * 変更されていないため、最新の revision を取得し直してから再試行する。
 * 同じリクエストの再送は安全で、適用済みの譜面は unchanged になる。
 */
export default defineEventHandler(async (event) => {
  const admin = await requireIrAdmin(event)
  const body = ((await readBody(event)) ?? {}) as ChartStatusBody
  return handleModerationAdmin(async () => {
    const results = await changeChartModeration({
      actorId: admin.id,
      items: parseChartModerationItems(body.items),
      status: parseChartModerationStatus(body.status),
      reason: parseReason(body.reason),
      note: parseNote(body.note),
    })
    return {
      results,
      summary: {
        changed: results.filter((item) => item.result === 'changed').length,
        unchanged: results.filter((item) => item.result === 'unchanged').length,
        conflict: results.filter((item) => item.result === 'conflict').length,
      },
    }
  })
})
