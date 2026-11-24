import { expect, test } from 'bun:test'
import { computed, ref } from 'vue'
import { useLocaleFormat } from '../app/composables/useLocaleFormat'

test('renders the same Japan time in SSR and browser time zones', () => {
  const originalTimeZone = process.env.TZ
  Object.assign(globalThis, { computed, useI18n: () => ({ locale: ref('ja') }) })
  try {
    const timestamp = '2026-11-24T16:05:06.000Z'
    for (const timeZone of ['UTC', 'America/Los_Angeles']) {
      process.env.TZ = timeZone
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe(timeZone)
      const format = useLocaleFormat()
      expect(format.formatDateAndTime(timestamp)).toEqual({ date: '2026/11/25', time: '1:05:06' })
      expect(format.formatDateTime(timestamp)).toBe('2026/11/25 1:05:06')
      expect(format.formatDate(timestamp, { dateStyle: 'medium' })).toBe('2026/11/25')
    }
  } finally {
    if (originalTimeZone === undefined) delete process.env.TZ
    else process.env.TZ = originalTimeZone
    Reflect.deleteProperty(globalThis, 'computed')
    Reflect.deleteProperty(globalThis, 'useI18n')
  }
})
