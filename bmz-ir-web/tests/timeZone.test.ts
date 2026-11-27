import { expect, jest, test } from 'bun:test'
import { dailyDateRange } from '../server/services/daily_report_core'
import { DAILY_BOUNDARY_MAX_MINUTES, DAILY_BOUNDARY_MIN_MINUTES } from '../shared/constants/ir'
import {
  dailyBoundaryForLocalTime,
  dailyBoundaryStart,
  formatClockMinutes,
  localDailyBoundary,
  normalizeTimeZone,
  timeZoneOffsetMinutes,
  watchDailyDate,
} from '../app/utils/timeZone'

const winter = Date.parse('2026-01-15T12:00:00Z')
const summer = Date.parse('2026-07-15T12:00:00Z')

test('accepts only time zones that Intl can use', () => {
  expect(normalizeTimeZone('America/Los_Angeles')).toBe('America/Los_Angeles')
  expect(normalizeTimeZone('UTC')).toBe('UTC')
  for (const value of ['', 'Mars/Olympus', 'x'.repeat(65), 9, null, undefined]) {
    expect(normalizeTimeZone(value)).toBeUndefined()
  }
})

test('reads UTC offsets including daylight saving time and partial hours', () => {
  expect(timeZoneOffsetMinutes('Asia/Tokyo', winter)).toBe(540)
  expect(timeZoneOffsetMinutes('America/Los_Angeles', winter)).toBe(-480)
  expect(timeZoneOffsetMinutes('America/Los_Angeles', summer)).toBe(-420)
  expect(timeZoneOffsetMinutes('Australia/Sydney', winter)).toBe(660)
  expect(timeZoneOffsetMinutes('Asia/Kathmandu', winter)).toBe(345)
  expect(timeZoneOffsetMinutes('UTC', winter + 999)).toBe(0)
})

test('local boundary choices start the result day on the same local date', () => {
  for (const timeZone of [
    'Asia/Tokyo',
    'America/Los_Angeles',
    'Pacific/Honolulu',
    'Europe/London',
    'Australia/Sydney',
    'Asia/Kathmandu',
    'Pacific/Kiritimati',
    'Etc/GMT+12',
  ]) {
    const offset = timeZoneOffsetMinutes(timeZone, winter)
    for (let localMinutes = 0; localMinutes < 24 * 60; localMinutes += 30) {
      const boundary = dailyBoundaryForLocalTime(localMinutes, offset)
      expect(boundary).toBeGreaterThanOrEqual(DAILY_BOUNDARY_MIN_MINUTES)
      expect(boundary).toBeLessThanOrEqual(DAILY_BOUNDARY_MAX_MINUTES)
      expect(localDailyBoundary(boundary, offset)).toEqual({ dayOffset: 0, minutes: localMinutes })

      const start = dailyDateRange('2026-01-15', boundary).start.getTime()
      expect(dailyBoundaryStart('2026-01-15', boundary)).toBe(start)
      const local = new Date(start + offset * 60_000)
      expect(local.toISOString().slice(0, 10)).toBe('2026-01-15')
      expect(local.getUTCHours() * 60 + local.getUTCMinutes()).toBe(localMinutes)
    }
  }
})

test('shows boundaries saved for another zone or season with their local day', () => {
  // The default 00:00 JST starts at 07:00 on the previous day in Los Angeles.
  expect(localDailyBoundary(0, -480)).toEqual({ dayOffset: -1, minutes: 7 * 60 })
  // 00:00 in Los Angeles in winter is 01:00 during daylight saving time.
  const boundary = dailyBoundaryForLocalTime(0, -480)
  expect(localDailyBoundary(boundary, -420)).toEqual({ dayOffset: 0, minutes: 60 })
  expect(localDailyBoundary(2699, 540)).toEqual({ dayOffset: 1, minutes: 20 * 60 + 59 })
  expect(formatClockMinutes(5 * 60 + 45)).toBe('05:45')
})

test('updates today at the configured boundary and after a suspended tab resumes', () => {
  const minutes = 25 * 60
  const start = dailyBoundaryStart('2026-01-15', minutes)
  jest.useFakeTimers({ now: start - 1 })
  const tab = Object.assign(new EventTarget(), { visibilityState: 'visible' })
  Object.assign(globalThis, { document: tab })
  const dates: string[] = []
  const stop = watchDailyDate(minutes, (date) => dates.push(date))
  try {
    expect(dates).toEqual(['2026-01-14'])
    jest.advanceTimersByTime(1)
    expect(dates).toEqual(['2026-01-14', '2026-01-15'])

    jest.setSystemTime(dailyBoundaryStart('2026-01-17', minutes) - 1)
    tab.dispatchEvent(new Event('visibilitychange'))
    expect(dates.at(-1)).toBe('2026-01-16')
    jest.advanceTimersByTime(1)
    expect(dates.at(-1)).toBe('2026-01-17')
    stop()
    tab.dispatchEvent(new Event('visibilitychange'))
    jest.advanceTimersByTime(86_400_000)
    expect(dates).toHaveLength(4)
  } finally {
    stop()
    jest.useRealTimers()
    Reflect.deleteProperty(globalThis, 'document')
  }
})
