import type {
  IrDailyBestSnapshot,
  IrDailyChartResult,
  IrDailyMode,
  IrDailyReport,
  IrDoubleOption,
  IrRuleMode,
} from '../../shared/types/ir'

const JST_OFFSET_MINUTES = 9 * 60
const DAY_MS = 24 * 60 * 60 * 1000

export interface DailyDateRange {
  start: Date
  end: Date
}

export interface DailyReportScoreRow {
  id: string
  chartSha256: string
  playedAt: Date | null
  serverReceivedAt: Date
  durationMs: number | null
  notes: number
  exScore: number
  clearType: string
  clearRank: number
  minBp: number
  lnPolicy: string
  doubleOption: IrDoubleOption
  ruleMode: IrRuleMode
  scoring: string
  chart: {
    md5: string | null
    title: string
    subtitle: string | null
    artist: string | null
    mode: string
    level: number | null
    difficulty: string | null
    notes: number
  }
}

export interface DailyReportBuildInput {
  player: IrDailyReport['player']
  date: string
  mode: IrDailyMode
  boundaryMinutes: number
  range: DailyDateRange
  /** 日開始前の対象譜面履歴と当日行。日終了後の行は無視される。 */
  history: DailyReportScoreRow[]
}

interface MutableBest {
  clear: { type: string; rank: number } | null
  exScore: number | null
  exScoreNotes: number | null
  minBp: number | null
}

interface MutableRule {
  rule: IrDailyChartResult['rules'][number]['rule']
  plays: number
  before: MutableBest
  after: MutableBest
}

interface MutableChart {
  chart: IrDailyChartResult['chart']
  firstEventAt: number
  rules: Map<string, MutableRule>
}

/** JST の指定日とユーザー設定の切り替わり時刻から半開区間 [start, end) を作る。 */
export function dailyDateRange(date: string, boundaryMinutes: number): DailyDateRange {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date)
  if (!match) {
    throw new Error('date must be YYYY-MM-DD')
  }
  if (!Number.isInteger(boundaryMinutes) || boundaryMinutes < 0 || boundaryMinutes > 1439) {
    throw new Error('daily boundary must be between 0 and 1439 minutes')
  }

  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  const utcMidnight = Date.UTC(year, month - 1, day)
  const parsed = new Date(utcMidnight)
  if (
    parsed.getUTCFullYear() !== year ||
    parsed.getUTCMonth() !== month - 1 ||
    parsed.getUTCDate() !== day
  ) {
    throw new Error('date is invalid')
  }

  const start = new Date(utcMidnight + (boundaryMinutes - JST_OFFSET_MINUTES) * 60_000)
  return { start, end: new Date(start.getTime() + DAY_MS) }
}

/** scores の履歴から日開始前と日終了時点の独立 best を構成する純粋関数。 */
export function buildDailyReport(input: DailyReportBuildInput): IrDailyReport {
  const startMs = input.range.start.getTime()
  const endMs = input.range.end.getTime()
  const dailyRows = input.history.filter((row) => {
    const time = scoreEventTime(row).getTime()
    return time >= startMs && time < endMs
  })
  const dailyChartHashes = new Set(dailyRows.map((row) => row.chartSha256))
  const charts = new Map<string, MutableChart>()

  for (const row of dailyRows) {
    const eventTime = scoreEventTime(row).getTime()
    const chart = charts.get(row.chartSha256) ?? {
      chart: {
        sha256: row.chartSha256,
        md5: row.chart.md5,
        title: row.chart.title,
        subtitle: row.chart.subtitle,
        artist: row.chart.artist ?? '',
        mode: row.chart.mode,
        level: row.chart.level,
        difficulty: row.chart.difficulty,
        notes: row.chart.notes,
      },
      firstEventAt: eventTime,
      rules: new Map<string, MutableRule>(),
    }
    chart.firstEventAt = Math.min(chart.firstEventAt, eventTime)
    const key = scoreRuleKey(row)
    const rule = chart.rules.get(key) ?? newRule(row)
    rule.plays += 1
    chart.rules.set(key, rule)
    charts.set(row.chartSha256, chart)
  }

  for (const row of input.history) {
    if (!dailyChartHashes.has(row.chartSha256)) {
      continue
    }
    const rule = charts.get(row.chartSha256)?.rules.get(scoreRuleKey(row))
    if (!rule) {
      continue
    }
    const eventTime = scoreEventTime(row).getTime()
    if (eventTime >= endMs) {
      continue
    }
    mergeBest(rule.after, row)
    if (eventTime < startMs) {
      mergeBest(rule.before, row)
    }
  }

  const playNotes = dailyRows.reduce((sum, row) => sum + row.notes, 0)
  const totalExScore = dailyRows.reduce((sum, row) => sum + row.exScore, 0)
  const knownDurations = dailyRows.filter((row) => row.durationMs !== null)

  return {
    player: input.player,
    date: input.date,
    mode: input.mode,
    timezone: 'Asia/Tokyo',
    boundary_minutes: input.boundaryMinutes,
    range: { start: input.range.start.toISOString(), end: input.range.end.toISOString() },
    summary: {
      play_notes: playNotes,
      clear_count: dailyRows.filter((row) => row.clearRank > 1).length,
      play_count: dailyRows.length,
      chart_count: dailyChartHashes.size,
      accuracy: playNotes > 0 ? (totalExScore / (playNotes * 2)) * 100 : null,
      play_time_ms: knownDurations.reduce((sum, row) => sum + (row.durationMs ?? 0), 0),
      play_time_unknown_count: dailyRows.length - knownDurations.length,
    },
    charts: [...charts.values()]
      .sort(
        (left, right) =>
          left.firstEventAt - right.firstEventAt ||
          left.chart.sha256.localeCompare(right.chart.sha256),
      )
      .map((chart) => ({
        chart: chart.chart,
        difficulty_labels: [],
        rules: [...chart.rules.values()].map((rule) => ({
          rule: rule.rule,
          plays: rule.plays,
          before: snapshot(rule.before),
          after: snapshot(rule.after),
          updated_fields: {
            clear: bestChanged(rule.before.clear?.rank ?? null, rule.after.clear?.rank ?? null),
            ex_score: bestChanged(rule.before.exScore, rule.after.exScore),
            min_bp: bestChanged(rule.before.minBp, rule.after.minBp),
          },
        })),
      })),
  }
}

function scoreEventTime(row: Pick<DailyReportScoreRow, 'playedAt' | 'serverReceivedAt'>): Date {
  return row.playedAt ?? row.serverReceivedAt
}

function scoreRuleKey(
  row: Pick<
    DailyReportScoreRow,
    'chartSha256' | 'lnPolicy' | 'doubleOption' | 'ruleMode' | 'scoring'
  >,
): string {
  return [row.chartSha256, row.lnPolicy, row.doubleOption, row.ruleMode, row.scoring].join('\0')
}

function newBest(): MutableBest {
  return { clear: null, exScore: null, exScoreNotes: null, minBp: null }
}

function newRule(row: DailyReportScoreRow): MutableRule {
  return {
    rule: {
      ln_policy: row.lnPolicy,
      double_option: row.doubleOption,
      rule_mode: row.ruleMode,
      scoring: row.scoring,
    },
    plays: 0,
    before: newBest(),
    after: newBest(),
  }
}

function mergeBest(best: MutableBest, row: DailyReportScoreRow): void {
  if (best.clear === null || row.clearRank > best.clear.rank) {
    best.clear = { type: row.clearType, rank: row.clearRank }
  }
  if (best.exScore === null || row.exScore > best.exScore) {
    best.exScore = row.exScore
    best.exScoreNotes = row.notes
  }
  best.minBp = best.minBp === null ? row.minBp : Math.min(best.minBp, row.minBp)
}

function snapshot(best: MutableBest): IrDailyBestSnapshot {
  return {
    clear: best.clear,
    ex_score: best.exScore,
    ex_score_notes: best.exScoreNotes,
    min_bp: best.minBp,
  }
}

function bestChanged(before: number | null, after: number | null): boolean {
  return after !== null && after !== before
}
