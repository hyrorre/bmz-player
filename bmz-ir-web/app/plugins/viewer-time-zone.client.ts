import {
  useViewerTimeZone,
  VIEWER_TIME_ZONE_COOKIE,
  viewerTimeZoneCookieOptions,
} from '../composables/useViewerTimeZone'
import { normalizeTimeZone } from '../utils/timeZone'

// hydration の後でブラウザのタイムゾーンを適用し、次回のSSRでも同じ時刻を描画できるよう保存する。
export default defineNuxtPlugin(() => {
  const timeZone = useViewerTimeZone()
  const cookie = useCookie<string | null>(VIEWER_TIME_ZONE_COOKIE, viewerTimeZoneCookieOptions)
  onNuxtReady(() => {
    const detected = normalizeTimeZone(Intl.DateTimeFormat().resolvedOptions().timeZone)
    if (!detected) return
    if (cookie.value !== detected) cookie.value = detected
    if (timeZone.value !== detected) timeZone.value = detected
  })
})
