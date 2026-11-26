import { afterEach, beforeEach, expect, test } from 'bun:test'
import { computed, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import { additional } from '../../i18n/locales/additional'
import { useDailyBoundaryLabel } from '../app/composables/useDailyBoundaryLabel'
import { useLocaleFormat } from '../app/composables/useLocaleFormat'
import {
  dailyBoundaryForLocalTime,
  formatClockMinutes,
  localDailyBoundary,
} from '../app/utils/timeZone'

const viewerTimeZone = ref('Asia/Tokyo')
const originalTimeZone = process.env.TZ

beforeEach(() => {
  // Nuxt injects these imports in the application build.
  Object.assign(globalThis, {
    computed,
    useI18n: () => ({ locale: ref('ja') }),
    useViewerTimeZone: () => viewerTimeZone,
  })
})

afterEach(() => {
  if (originalTimeZone === undefined) delete process.env.TZ
  else process.env.TZ = originalTimeZone
  for (const name of ['computed', 'useI18n', 'useViewerTimeZone']) {
    Reflect.deleteProperty(globalThis, name)
  }
})

test('formats instants in the viewer time zone regardless of the runtime time zone', () => {
  const timestamp = '2026-11-24T16:05:06.000Z'
  for (const [timeZone, date, time] of [
    ['Asia/Tokyo', '2026/11/25', '1:05:06'],
    ['America/Los_Angeles', '2026/11/24', '8:05:06'],
    ['Australia/Sydney', '2026/11/25', '3:05:06'],
  ] as const) {
    viewerTimeZone.value = timeZone
    for (const runtimeTimeZone of ['UTC', 'America/New_York']) {
      process.env.TZ = runtimeTimeZone
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe(runtimeTimeZone)
      const format = useLocaleFormat()
      expect(format.formatDateAndTime(timestamp)).toEqual({ date, time })
      expect(format.formatDateTime(timestamp)).toBe(`${date} ${time}`)
      expect(format.formatDate(timestamp, { dateStyle: 'medium' })).toBe(date)
    }
  }
})

test('keeps an explicit time zone and names the viewer time zone for boundaries', () => {
  viewerTimeZone.value = 'America/Los_Angeles'
  const format = useLocaleFormat()
  expect(format.formatDate('2026-11-25T00:00:00Z', { dateStyle: 'medium', timeZone: 'UTC' })).toBe(
    '2026/11/25',
  )
  expect(format.formatTimeWithZone('2026-11-25T08:00:00Z')).toBe('0:00 GMT-8')
  viewerTimeZone.value = 'Asia/Tokyo'
  expect(format.formatTimeWithZone('2026-11-25T08:00:00Z')).toBe('17:00 JST')
})

test('labels result days across the date line with the actual local day offset', () => {
  const i18n = createI18n({ legacy: false, locale: 'ja', messages: { ja: additional.ja.messages } })
  Object.assign(globalThis, { useI18n: () => i18n.global })
  const label = useDailyBoundaryLabel()
  const samoa = -11 * 60
  const kiritimati = 14 * 60
  const next = localDailyBoundary(dailyBoundaryForLocalTime(23 * 60 + 30, samoa), kiritimati)
  expect(label(formatClockMinutes(next.minutes), next.dayOffset)).toBe('2日後 00:30')
  const previous = localDailyBoundary(dailyBoundaryForLocalTime(0, kiritimati), samoa)
  expect(label(formatClockMinutes(previous.minutes), previous.dayOffset)).toBe('2日前 23:00')
  expect(label('15:00', -1)).toBe('前日 15:00')
  expect(label('15:00', 0)).toBe('15:00')
  expect(label('15:00', 1)).toBe('翌日 15:00')
})
