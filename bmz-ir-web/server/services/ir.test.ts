import { describe, expect, test } from 'bun:test'
import { isUniqueConstraintError } from '../utils/db_errors'
import { computeCourseHash, validateCourseScoreSubmission } from './course_ir'
import { __test, stableStringify } from './ir'

describe('stableStringify', () => {
  test('matches JCS number formatting used by Rust IR evidence', () => {
    const value = {
      numbers: [333333333.33333329, 1e30, 4.5, 2e-3, 1e-27, 1e-6, 1e-7, -0],
      chart: {
        total: 160.0,
        bpm: {
          min: 120.0,
          max: 120.5,
        },
      },
    }

    expect(stableStringify(value)).toBe(
      '{"chart":{"bpm":{"max":120.5,"min":120},"total":160},"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27,0.000001,1e-7,0]}',
    )
  })

  test('sorts keys by UTF-16 code units', () => {
    expect(
      stableStringify({
        '\u{e000}': 2,
        '\u{10000}': 1,
      }),
    ).toBe('{"𐀀":1,"":2}')
  })

  test('rejects values outside canonical JSON', () => {
    expect(() => stableStringify(undefined)).toThrow()
    expect(() => stableStringify(Number.NaN)).toThrow()
    expect(() => stableStringify(Number.POSITIVE_INFINITY)).toThrow()
  })
})

describe('ranking best row aggregation', () => {
  test('reads arrange options from new and legacy play options', () => {
    expect(
      __test.arrangeOptionsFromPlayOptions({
        option: 'random',
        arrange_1p: 'f-random',
        arrange_2p: 'mf-random',
      }),
    ).toEqual({ arrange_1p: 'f-random', arrange_2p: 'mf-random' })

    expect(__test.arrangeOptionsFromPlayOptions({ option: 'random' })).toEqual({
      arrange_1p: 'random',
      arrange_2p: undefined,
    })
  })

  test('deduplicates users and keeps independent display bests', () => {
    const rows = [
      rankingRow({
        player_id: 'player-1',
        score_id: 'hard-score',
        ex_score: 2000,
        clear_type: 'Hard',
        clear_rank: 5,
        max_combo: 900,
        min_bp: 20,
      }),
      rankingRow({
        player_id: 'player-1',
        score_id: 'fc-score',
        ex_score: 1900,
        clear_type: 'FullCombo',
        clear_rank: 7,
        max_combo: 1000,
        min_bp: 0,
      }),
      rankingRow({
        player_id: 'player-2',
        score_id: 'other-score',
        ex_score: 1950,
      }),
    ]

    const deduped = __test.dedupeBestRowsByPlayer(rows)

    expect(deduped).toHaveLength(2)
    const player = deduped.find((row) => row.player_id === 'player-1')
    expect(player?.score_id).toBe('hard-score')
    expect(player?.ex_score).toBe(2000)
    expect(player?.clear_type).toBe('FullCombo')
    expect(player?.best_clear_score_id).toBe('fc-score')
    expect(player?.max_combo).toBe(1000)
    expect(player?.min_bp).toBe(0)
  })

  test('rebuilds aggregate best rows from score history', () => {
    const rebuilt = __test.bestRowsFromHistory([
      {
        ...rankingRow({ score_id: 'hard-score', ex_score: 2000, clear_type: 'Hard' }),
        id: 'hard-score',
      },
      {
        ...rankingRow({
          score_id: 'fc-score',
          ex_score: 1900,
          clear_type: 'FullCombo',
          clear_rank: 7,
          max_combo: 1000,
          min_bp: 0,
        }),
        id: 'fc-score',
      },
    ])

    expect(rebuilt).toHaveLength(1)
    expect(rebuilt[0]?.score_id).toBe('hard-score')
    expect(rebuilt[0]?.clear_type).toBe('FullCombo')
    expect(rebuilt[0]?.best_clear_score_id).toBe('fc-score')
  })
})

describe('database error classification', () => {
  test('detects D1 unique constraint errors wrapped by drizzle', () => {
    const cause = new Error(
      'D1_ERROR: UNIQUE constraint failed: scores.player_id, scores.idempotency_key: SQLITE_CONSTRAINT',
    )
    const error = new Error('Failed query: insert into "scores" ...', { cause })

    expect(isUniqueConstraintError(error)).toBe(true)
  })

  test('ignores non-constraint query errors', () => {
    const cause = new Error('D1_ERROR: database is locked')
    const error = new Error('Failed query: select * from scores', { cause })

    expect(isUniqueConstraintError(error)).toBe(false)
  })

  test('ignores foreign key constraint errors', () => {
    const cause = new Error('D1_ERROR: FOREIGN KEY constraint failed: SQLITE_CONSTRAINT')
    const error = new Error('Failed query: insert into "best_course_scores" ...', { cause })

    expect(isUniqueConstraintError(error)).toBe(false)
  })
})

describe('course score validation', () => {
  test('normalizes legacy snake_case ln policy settings', () => {
    const payload = baseCourseSubmission({ lnPolicy: 'auto_hcn' })

    const validated = validateCourseScoreSubmission(payload)

    expect(validated.rule.ln_policy).toBe('AutoHcn')
  })

  test('keeps canonical ln policy settings', () => {
    const payload = baseCourseSubmission({ lnPolicy: 'ForceCn' })

    const validated = validateCourseScoreSubmission(payload)

    expect(validated.rule.ln_policy).toBe('ForceCn')
  })

  test('rejects invalid course ln policy settings', () => {
    const payload = baseCourseSubmission({ lnPolicy: 'auto' })

    expect(() => validateCourseScoreSubmission(payload)).toThrow('rule.ln_policy is invalid')
  })

  test('rejects invalid course rule modes', () => {
    const payload = baseCourseSubmission()
    payload.rule.rule_mode = 'Unknown' as never

    expect(() => validateCourseScoreSubmission(payload)).toThrow('rule_mode is invalid')
  })
})

function rankingRow(overrides: Partial<ReturnType<typeof baseRankingRow>> = {}) {
  const scoreId = overrides.score_id ?? 'score-1'
  return {
    ...baseRankingRow(),
    best_ex_score_id: scoreId,
    best_clear_score_id: scoreId,
    best_max_combo_score_id: scoreId,
    best_min_bp_score_id: scoreId,
    best_min_cb_score_id: scoreId,
    ...overrides,
  }
}

function baseRankingRow() {
  return {
    player_id: 'player-1',
    chart_sha256: 'a'.repeat(64),
    score_id: 'score-1',
    best_ex_score_id: 'score-1',
    best_clear_score_id: 'score-1',
    best_max_combo_score_id: 'score-1',
    best_min_bp_score_id: 'score-1',
    best_min_cb_score_id: 'score-1',
    ex_score: 1000,
    clear_type: 'Normal',
    clear_rank: 4,
    max_combo: 500,
    min_bp: 30,
    min_cb: 25,
    server_received_at: new Date('2026-01-01T00:00:00Z'),
    gauge: 'Normal',
    ln_policy: 'AutoLn' as const,
    effective_ln_mode: 'ln' as const,
    double_option: 'off' as const,
    rule_mode: 'Beatoraja' as const,
    scoring: 'bms_ex_score_v1' as const,
    device_type: 'keyboard' as const,
    played_at: '2026-01-01T00:00:00.000Z',
    verification: 'unverified' as const,
  }
}

function baseCourseSubmission({ lnPolicy = 'AutoLn' }: { lnPolicy?: string } = {}) {
  const charts = ['a'.repeat(64), 'b'.repeat(64)]
  const constraints = { gauge: 'Class', ln: 'Off' }
  return {
    client: { name: 'BMZ', version: '0.1.0', platform: 'windows' },
    course: {
      course_hash: computeCourseHash(charts, constraints),
      title: 'Dan 1',
      kind: 'dan' as const,
      charts,
      constraints,
    },
    rule: {
      gauge: 'Class',
      ln_policy: lnPolicy,
      rule_mode: 'Beatoraja' as const,
      scoring: 'bms_ex_score_v1' as const,
    },
    result: {
      clear: 'Normal',
      course_clear: true,
      course_failed: false,
      played_entries: 2,
      ex_score: 320,
      max_ex_score: 400,
      max_combo: 200,
      bp: 4,
      judges: {
        pgreat: 150,
        great: 20,
        good: 0,
        bad: 2,
        poor: 2,
        empty_poor: 0,
      },
      gauge_value: 78,
      entries: [
        {
          sha256: charts[0],
          ex_score: 160,
          max_combo: 100,
          bp: 2,
          clear: 'Normal',
          gauge_end: 62,
        },
        {
          sha256: charts[1],
          ex_score: 160,
          max_combo: 100,
          bp: 2,
          clear: 'Normal',
          gauge_end: 78,
        },
      ],
      played_at: 1_767_225_600,
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: 'course-test',
  }
}
