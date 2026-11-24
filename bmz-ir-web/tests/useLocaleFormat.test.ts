import { afterEach, beforeEach, expect, test } from 'bun:test'
import { computed, ref } from 'vue'
import { useLocaleFormat } from '../app/composables/useLocaleFormat'

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
