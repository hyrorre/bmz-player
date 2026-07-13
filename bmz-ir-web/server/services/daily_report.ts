import { and, eq, gte, inArray, isNotNull, isNull, lt, or } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import type { IrDailyMode, IrDailyReport } from '../../shared/types/ir'
import { buildDailyReport, dailyDateRange, type DailyReportScoreRow } from './daily_report_core'
import { lookupDifficultyLabels } from './difficulty_tables'

const BASELINE_CHART_CHUNK_SIZE = 50

const scoreSelection = {
  id: schema.scores.id,
  chartSha256: schema.scores.chartSha256,
  playedAt: schema.scores.playedAt,
  serverReceivedAt: schema.scores.serverReceivedAt,
  durationMs: schema.scores.durationMs,
  notes: schema.scores.notes,
  exScore: schema.scores.exScore,
  clearType: schema.scores.clearType,
  clearRank: schema.scores.clearRank,
  minBp: schema.scores.minBp,
  lnPolicy: schema.scores.lnPolicy,
  doubleOption: schema.scores.doubleOption,
  ruleMode: schema.scores.ruleMode,
  scoring: schema.scores.scoring,
  chart: {
    md5: schema.charts.md5,
    title: schema.charts.title,
    subtitle: schema.charts.subtitle,
    artist: schema.charts.artist,
    mode: schema.charts.mode,
    level: schema.charts.level,
    difficulty: schema.charts.difficulty,
    notes: schema.charts.notes,
  },
}

export { dailyDateRange } from './daily_report_core'

export async function loadDailyReport(input: {
  playerId: string
  date: string
  mode: IrDailyMode
}): Promise<IrDailyReport | null> {
  const profiles = await db
    .select({
      id: schema.profiles.id,
      displayName: schema.profiles.displayName,
      boundaryMinutes: schema.profiles.dailyBoundaryMinutes,
    })
    .from(schema.profiles)
    .where(eq(schema.profiles.id, input.playerId))
    .limit(1)
  const profile = profiles[0]
  if (!profile) {
    return null
  }

  const boundaryMinutes = profile.boundaryMinutes
  const range = dailyDateRange(input.date, boundaryMinutes)
  const occurredDuringDay = or(
    and(
      isNotNull(schema.scores.playedAt),
      gte(schema.scores.playedAt, range.start),
      lt(schema.scores.playedAt, range.end),
    ),
    and(
      isNull(schema.scores.playedAt),
      gte(schema.scores.serverReceivedAt, range.start),
      lt(schema.scores.serverReceivedAt, range.end),
    ),
  )
  const dailyRows = (await db
    .select(scoreSelection)
    .from(schema.scores)
    .innerJoin(schema.charts, eq(schema.charts.sha256, schema.scores.chartSha256))
    .where(
      and(
        eq(schema.scores.playerId, input.playerId),
        eq(schema.scores.accepted, true),
        occurredDuringDay,
      ),
    )) as DailyReportScoreRow[]

  const chartHashes = [...new Set(dailyRows.map((row) => row.chartSha256))]
  const baselineRows: DailyReportScoreRow[] = []
  const occurredBeforeDay = or(
    and(isNotNull(schema.scores.playedAt), lt(schema.scores.playedAt, range.start)),
    and(isNull(schema.scores.playedAt), lt(schema.scores.serverReceivedAt, range.start)),
  )
  for (let offset = 0; offset < chartHashes.length; offset += BASELINE_CHART_CHUNK_SIZE) {
    const chunk = chartHashes.slice(offset, offset + BASELINE_CHART_CHUNK_SIZE)
    const rows = await db
      .select(scoreSelection)
      .from(schema.scores)
      .innerJoin(schema.charts, eq(schema.charts.sha256, schema.scores.chartSha256))
      .where(
        and(
          eq(schema.scores.playerId, input.playerId),
          eq(schema.scores.accepted, true),
          inArray(schema.scores.chartSha256, chunk),
          occurredBeforeDay,
        ),
      )
    baselineRows.push(...(rows as DailyReportScoreRow[]))
  }

  const report = buildDailyReport({
    player: { id: profile.id, display_name: profile.displayName },
    date: input.date,
    mode: input.mode,
    boundaryMinutes,
    range,
    history: [...baselineRows, ...dailyRows],
  })
  const difficultyLabels = await lookupDifficultyLabels(
    report.charts.map(({ chart }) => ({ sha256: chart.sha256, md5: chart.md5 })),
  )
  for (const chart of report.charts) {
    chart.difficulty_labels = difficultyLabels.get(chart.chart.sha256) ?? []
  }
  return report
}
