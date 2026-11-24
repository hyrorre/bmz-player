import { JST_UTC_OFFSET_MINUTES } from '../../shared/constants/ir'

/** ブラウザのタイムゾーンを受け取る前（初回のSSRなど）に使うタイムゾーン。 */
export const DEFAULT_TIME_ZONE = 'Asia/Tokyo'

const MINUTE_MS = 60_000
const DAY_MINUTES = 24 * 60
const partFormatters = new Map<string, Intl.DateTimeFormat>()

/** IANA タイムゾーン名を正規化する。Intl が扱えない値は undefined。 */
export function normalizeTimeZone(value: unknown): string | undefined {
  if (typeof value !== 'string' || !value || value.length > 64) return undefined
  try {
    return new Intl.DateTimeFormat('en-US', { timeZone: value }).resolvedOptions().timeZone
  } catch {
    return undefined
  }
}

/** 指定時刻の UTC からの差（分）。夏時間を含む。 */
export function timeZoneOffsetMinutes(timeZone: string, time: number): number {
  let formatter = partFormatters.get(timeZone)
  if (!formatter) {
    formatter = new Intl.DateTimeFormat('en-US', {
      timeZone,
      hourCycle: 'h23',
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: 'numeric',
      second: 'numeric',
    })
    partFormatters.set(timeZone, formatter)
  }
  const parts = Object.fromEntries(
    formatter.formatToParts(time).map(({ type, value }) => [type, Number(value)]),
  )
  const wallClock = Date.UTC(
    parts.year!,
    parts.month! - 1,
    parts.day!,
    parts.hour!,
    parts.minute!,
    parts.second!,
  )
  return Math.round((wallClock - (time - (((time % 1000) + 1000) % 1000))) / MINUTE_MS)
}

/** 成果日 date の開始時刻。保存値は JST 00:00 からの分。 */
export function dailyBoundaryStart(date: string, boundaryMinutes: number): number {
  return Date.parse(`${date}T00:00:00+09:00`) + boundaryMinutes * MINUTE_MS
}

/**
 * 保存済みの切り替わりを、UTC との差が offsetMinutes の地域の時刻にする。
 * dayOffset は成果日の日付に対する現地の日付のずれ（前日なら -1）。
 */
export function localDailyBoundary(boundaryMinutes: number, offsetMinutes: number) {
  const local = boundaryMinutes - JST_UTC_OFFSET_MINUTES + offsetMinutes
  const dayOffset = Math.floor(local / DAY_MINUTES)
  return { dayOffset, minutes: local - dayOffset * DAY_MINUTES }
}

/** 現地の localMinutes に成果日が切り替わる保存値。成果日の日付は現地の日付になる。 */
export function dailyBoundaryForLocalTime(localMinutes: number, offsetMinutes: number): number {
  return localMinutes + JST_UTC_OFFSET_MINUTES - offsetMinutes
}

/** 0 時からの分を HH:MM にする。 */
export function formatClockMinutes(minutes: number): string {
  const hours = Math.floor(minutes / 60)
  return `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`
}
