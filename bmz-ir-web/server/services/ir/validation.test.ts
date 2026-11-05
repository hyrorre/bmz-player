import { describe, expect, test } from 'bun:test'
import type { IrScoreSubmission } from '../../../shared/types/ir'
import { validateScoreSubmission } from './common'

function submission(): IrScoreSubmission {
  const counts = { pgreat: 0, great: 0, good: 0, bad: 0, poor: 0, empty_poor: 0 }
  return {
    client: { name: 'BMZ', version: 'test', platform: 'linux' },
    chart: { sha256: 'a'.repeat(64) },
    rule: {
      play_mode: 'normal',
      key_mode: '7k',
      gauge: 'normal',
      ln_policy: 'ForceLn',
      effective_ln_mode: 'ln',
      rule_mode: 'Beatoraja',
      judge_algorithm: 'combo',
      scoring: 'bms_ex_score_v1',
    },
    result: {
      clear: 'Failed',
      played_at: 1234567890,
      notes: 100,
      ex_score: 70,
      max_combo: 30,
      min_bp: 1,
      min_cb: 1,
      pass_notes: 45,
      judges: { fast: { ...counts, pgreat: 30, great: 10, bad: 5, empty_poor: 200 }, slow: counts },
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: 'validation-test',
  }
}

describe('score submission consistency', () => {
  test('accepts partial failed plays, empty POOR and independent imported BP/CB minima', () => {
    for (const source of ['local', 'lr2', 'beatoraja']) {
      const payload = submission()
      payload.play_options.source_kind = source
      payload.play_options.submission_source = 'local_backfill'
      expect(validateScoreSubmission(payload)).toBe(payload)
    }
  })

  test.each([
    ['ex_score', 1_000_000_000],
    ['ex_score', 69],
    ['max_combo', 101],
    ['pass_notes', 101],
    ['notes', 40],
    ['min_bp', Number.MAX_SAFE_INTEGER + 1],
    ['min_cb', -1],
    ['ex_score', 0.5],
  ] as const)('rejects inconsistent %s=%s', (key, value) => {
    const payload = submission()
    payload.result[key] = value
    expect(() => validateScoreSubmission(payload)).toThrow()
  })

  test('rejects extra judged notes and forged EX scores even for backfill', () => {
    const payload = submission()
    payload.result.judges.slow.poor = 100
    expect(() => validateScoreSubmission(payload)).toThrow('result.judges exceeds')
    payload.result.judges.slow.poor = 0
    payload.play_options.submission_source = 'local_backfill'
    payload.play_options.source_kind = 'lr2'
    payload.result.ex_score = 200
    expect(() => validateScoreSubmission(payload)).toThrow('does not match')
  })
})
