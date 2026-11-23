import { and, asc, count, eq, isNotNull, max, sql } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { IR_SCORE_GRADES } from '../../shared/constants/ir'
import type {
  IrDifficultyDistribution,
  IrDistributionCharts,
  IrDistributionMode,
} from '../../shared/types/ir'
import { difficultyTableCharts } from './difficulty_tables'
import { isPublicPlayer, publicPlayerCondition } from './moderation/visibility'

export async function loadDifficultyDistribution(
  playerId: string,
  mode: IrDistributionMode,
  tableId?: string,
): Promise<IrDifficultyDistribution | null> {
  const profiles = schema.profiles
  const [player] = await db
    .select({ id: profiles.id, display_name: profiles.displayName })
    .from(profiles)
    .where(and(eq(profiles.id, playerId), publicPlayerCondition(profiles.id)))
  if (!player) return null

  const tables = schema.difficultyTables
  const activeTables = await db
    .select({
      id: tables.id,
      name: tables.name,
      symbol: tables.symbol,
      levelOrder: tables.levelOrder,
    })
    .from(tables)
    .where(isNotNull(tables.activeGeneration))
    .orderBy(asc(tables.priority), asc(tables.name), asc(tables.id))
  const table = activeTables.find(({ id }) => id === tableId) ?? activeTables[0]
  if (!table) return { player, tables: [], table: null, levels: [] }

  const ranks = chartRanks(playerId, mode, table.id)
  const rows = await db
    .select({ level: ranks.level, rank: ranks.rank, count: count() })
    .from(ranks)
    .groupBy(ranks.level, sql`${ranks.rank}`)
    .orderBy(sql`${ranks.rank} desc`)

  type Level = IrDifficultyDistribution['levels'][number]
  const levels = new Map<string, Level>()
  for (const row of rows) {
    const level = levels.get(row.level) ?? { level: row.level, total: 0, segments: [] }
    level.segments.push({ rank: row.rank, count: row.count })
    level.total += row.count
    levels.set(row.level, level)
  }
  const order = new Map(table.levelOrder.map((level, i) => [level, i]))
  const unlisted = table.levelOrder.length
  const collator = new Intl.Collator('en', { numeric: true })
  return {
    player,
    tables: activeTables.map(({ levelOrder: _levelOrder, ...item }) => item),
    table: { id: table.id, name: table.name, symbol: table.symbol },
    levels: [...levels.values()].sort(
      (left, right) =>
        (order.get(left.level) ?? unlisted) - (order.get(right.level) ?? unlisted) ||
        collator.compare(left.level, right.level) ||
        left.level.localeCompare(right.level),
    ),
  }
}

export async function loadDistributionCharts(
  playerId: string,
  options: {
    mode: IrDistributionMode
    table: string
    level: string
    rank: number
    limit: number
    offset: number
  },
): Promise<IrDistributionCharts | null> {
  if (!(await isPublicPlayer(playerId))) return null
  const { mode, table, level, rank, limit, offset } = options
  const ranks = chartRanks(playerId, mode, table, level)
  const charts = schema.charts
  const [items, counts] = await Promise.all([
    db
      .select({
        sha256: charts.sha256,
        title: charts.title,
        subtitle: charts.subtitle,
        artist: charts.artist,
      })
      .from(ranks)
      .innerJoin(charts, eq(charts.sha256, ranks.sha256))
      .where(eq(ranks.rank, rank))
      .orderBy(sql`${charts.title} collate nocase`, charts.subtitle, charts.sha256)
      .limit(limit)
      .offset(offset),
    db.select({ total: count() }).from(ranks).where(eq(ranks.rank, rank)),
  ])
  const total = counts[0]?.total ?? 0
  return { charts: items, pagination: { limit, offset, total, has_more: offset + limit < total } }
}

function chartRanks(playerId: string, mode: IrDistributionMode, tableId: string, level?: string) {
  const members = difficultyTableCharts(tableId, level)
  const highestRank =
    mode === 'score'
      ? highestScoreRank(playerId, members.sha256)
      : highestLamp(playerId, members.sha256)
  return db
    .select({
      sha256: members.sha256,
      level: members.level,
      rank: sql<number>`coalesce((${highestRank}), 0)`.as('rank_index'),
    })
    .from(members)
    .as('chart_ranks')
}

type ChartHash = ReturnType<typeof difficultyTableCharts>['sha256']

function highestLamp(playerId: string, chartSha256: ChartHash) {
  const best = schema.bestScores
  return db
    .select({ rank: max(best.clearRank) })
    .from(best)
    .where(
      and(
        eq(best.playerId, playerId),
        eq(best.chartSha256, chartSha256),
        eq(best.scoring, 'bms_ex_score_v1'),
        eq(best.doubleOption, 'off'),
      ),
    )
}

function highestScoreRank(playerId: string, chartSha256: ChartHash) {
  const score = schema.scores
  const maxEx = sql`(${score.notes} * 2)`
  // The cast keeps the 2/9 through 9/9 thresholds exact when D1 binds the multiplier as REAL.
  const scoreRank = sql<number>`max(1, cast(${score.exScore} * ${IR_SCORE_GRADES.length} / ${maxEx} as integer))`
  return db
    .select({ rank: max(scoreRank) })
    .from(score)
    .where(
      and(
        eq(score.playerId, playerId),
        eq(score.chartSha256, chartSha256),
        eq(score.scoring, 'bms_ex_score_v1'),
        eq(score.doubleOption, 'off'),
        eq(score.accepted, true),
        sql`${score.notes} > 0 and ${score.exScore} between 0 and ${maxEx}`,
      ),
    )
}
