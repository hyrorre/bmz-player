import { DEFAULT_TIME_ZONE, normalizeTimeZone } from '../utils/timeZone'

export const VIEWER_TIME_ZONE_COOKIE = 'ir-time-zone'
export const viewerTimeZoneCookieOptions = {
  maxAge: 60 * 60 * 24 * 365,
  sameSite: 'lax',
} as const

/**
 * 時刻の表示に使う閲覧者のタイムゾーン。
 * SSR とhydrationはブラウザが保存したCookieの値（初回は日本時間）で描画し、
 * 表示後にブラウザのタイムゾーンへ切り替える。
 */
export function useViewerTimeZone() {
  return useState(
    'ir-viewer-time-zone',
    () =>
      normalizeTimeZone(useCookie(VIEWER_TIME_ZONE_COOKIE, viewerTimeZoneCookieOptions).value) ??
      DEFAULT_TIME_ZONE,
  )
}
