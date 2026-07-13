import { describe, expect, test } from 'bun:test'
import { buildDailyReport, dailyDateRange, type DailyReportScoreRow } from './daily_report_core'

describe('dailyDateRange', () => {
  test('JST の境界時刻を UTC の半開区間へ変換する', () => {
    const range = dailyDateRange('2026-07-13', 4 * 60)

    expect(range.start.toISOString()).toBe('2026-07-12T19:00:00.000Z')
    expect(range.end.toISOString()).toBe('2026-07-13T19:00:00.000Z')
  })

  test('存在しない日付と範囲外の境界を拒否する', () => {
    expect(() => dailyDateRange('2026-02-29', 0)).toThrow('date is invalid')
    expect(() => dailyDateRange('2026-07-13', 1440)).toThrow(
      'daily boundary must be between 0 and 1439 minutes',
    )
  })
})

describe('buildDailyReport', () => {
  test('played_at を優先し、日開始前と日終了時点の best を指標ごとに集約する', () => {
    const range = dailyDateRange('2026-07-13', 4 * 60)
    const history = [
      scoreRow({
        id: 'before',
        playedAt: new Date('2026-07-12T18:00:00.000Z'),
        serverReceivedAt: new Date('2026-07-12T18:01:00.000Z'),
        exScore: 100,
        clearType: 'Normal',
        clearRank: 4,
        minBp: 20,
      }),
      // 受信は成果日内だが played_at が日開始前なので、当日プレイには数えない。
      scoreRow({
        id: 'backfill',
        playedAt: new Date('2026-07-12T18:30:00.000Z'),
        serverReceivedAt: new Date('2026-07-12T20:00:00.000Z'),
        exScore: 150,
        clearType: 'Normal',
        clearRank: 4,
        minBp: 20,
      }),
      scoreRow({
        id: 'daily-clear',
        playedAt: new Date('2026-07-12T19:00:00.000Z'),
        serverReceivedAt: new Date('2026-07-12T19:00:01.000Z'),
        durationMs: 1_000,
        notes: 100,
        exScore: 90,
        clearType: 'Hard',
        clearRank: 5,
        minBp: 30,
      }),
      scoreRow({
        id: 'daily-bp',
        playedAt: new Date('2026-07-12T20:00:00.000Z'),
        serverReceivedAt: new Date('2026-07-12T20:00:01.000Z'),
        durationMs: null,
        notes: 100,
        exScore: 120,
        clearType: 'Failed',
        clearRank: 1,
        minBp: 10,
      }),
      scoreRow({
        id: 'fallback',
        chartSha256: 'b'.repeat(64),
        playedAt: null,
        serverReceivedAt: new Date('2026-07-13T18:59:59.999Z'),
        durationMs: 500,
        notes: 50,
        exScore: 80,
        clearType: 'Normal',
        clearRank: 4,
        minBp: 5,
        chart: { title: 'Second chart' },
      }),
      scoreRow({
        id: 'at-end',
        chartSha256: 'c'.repeat(64),
        playedAt: null,
        serverReceivedAt: new Date('2026-07-13T19:00:00.000Z'),
      }),
    ]

    const report = buildDailyReport({
      player: { id: 'player', display_name: 'Player' },
      date: '2026-07-13',
      mode: 'all',
      boundaryMinutes: 240,
      range,
      history,
    })

    expect(report.summary).toEqual({
      play_notes: 250,
      clear_count: 2,
      play_count: 3,
      chart_count: 2,
      accuracy: expect.any(Number),
      play_time_ms: 1_500,
      play_time_unknown_count: 1,
    })
    expect(report.summary.accuracy).toBeCloseTo(58)
    expect(report.charts).toHaveLength(2)

    const firstRule = report.charts[0]?.rules[0]
    expect(firstRule?.plays).toBe(2)
    expect(firstRule?.before).toEqual({
      clear: { type: 'Normal', rank: 4 },
      ex_score: 150,
      ex_score_notes: 100,
      min_bp: 20,
    })
    expect(firstRule?.after).toEqual({
      clear: { type: 'Hard', rank: 5 },
      ex_score: 150,
      ex_score_notes: 100,
      min_bp: 10,
    })
    expect(firstRule?.updated_fields).toEqual({ clear: true, ex_score: false, min_bp: true })

    const fallbackRule = report.charts[1]?.rules[0]
    expect(fallbackRule?.before).toEqual({
      clear: null,
      ex_score: null,
      ex_score_notes: null,
      min_bp: null,
    })
    expect(fallbackRule?.after).toEqual({
      clear: { type: 'Normal', rank: 4 },
      ex_score: 80,
      ex_score_notes: 50,
      min_bp: 5,
    })
  })
})

function scoreRow(
  overrides: Partial<Omit<DailyReportScoreRow, 'chart'>> & {
    chart?: Partial<DailyReportScoreRow['chart']>
  },
): DailyReportScoreRow {
  return {
    id: overrides.id ?? 'score',
    chartSha256: overrides.chartSha256 ?? 'a'.repeat(64),
    playedAt:
      overrides.playedAt === undefined ? new Date('2026-07-12T19:00:00.000Z') : overrides.playedAt,
    serverReceivedAt: overrides.serverReceivedAt ?? new Date('2026-07-12T19:00:01.000Z'),
    durationMs: overrides.durationMs === undefined ? 1_000 : overrides.durationMs,
    notes: overrides.notes ?? 100,
    exScore: overrides.exScore ?? 100,
    clearType: overrides.clearType ?? 'Normal',
    clearRank: overrides.clearRank ?? 4,
    minBp: overrides.minBp ?? 10,
    lnPolicy: overrides.lnPolicy ?? 'AutoLn',
    doubleOption: overrides.doubleOption ?? 'off',
    ruleMode: overrides.ruleMode ?? 'Beatoraja',
    scoring: overrides.scoring ?? 'bms_ex_score_v1',
    chart: {
      md5: overrides.chart?.md5 ?? null,
      title: overrides.chart?.title ?? 'Chart',
      subtitle: overrides.chart?.subtitle ?? null,
      artist: overrides.chart?.artist ?? 'Artist',
      mode: overrides.chart?.mode ?? 'Beat7K',
      level: overrides.chart?.level ?? 12,
      difficulty: overrides.chart?.difficulty ?? 'Another',
      notes: overrides.chart?.notes ?? 100,
    },
  }
}
