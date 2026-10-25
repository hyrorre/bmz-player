import { and, asc, desc, eq, sql, type SQL } from 'drizzle-orm'
import { db, schema } from 'hub:db'

/** player 単位の best 行を順位順に返す。limit 省略時は全行。 */
export async function fetchCourseRankingRows(conditions: SQL[], limit?: number) {
  const scores = schema.courseScores
  const ranked = db
    .select({
      player_id: scores.playerId,
      course_score_id: scores.id,
      ex_score: scores.exScore,
      clear_type: scores.clearType,
      clear_rank: scores.clearRank,
      course_clear: scores.courseClear,
      max_combo: scores.maxCombo,
      bp: scores.bp,
      device_type: scores.deviceType,
      rule_mode: scores.ruleMode,
      played_at: scores.playedAt,
      server_received_at: scores.serverReceivedAt,
      verification: scores.verification,
      player_position: sql<number>`row_number() over (partition by ${scores.playerId}
      order by ${scores.exScore} desc, ${scores.clearRank} desc, ${scores.bp} asc,
      ${scores.maxCombo} desc, ${scores.serverReceivedAt} desc, ${scores.id} asc)`.as(
        'player_position',
      ),
    })
    .from(scores)
    .where(and(...conditions))
    .as('ranked')
  const query = db
    .select()
    .from(ranked)
    .where(eq(ranked.player_position, 1))
    .orderBy(
      desc(ranked.ex_score),
      desc(ranked.clear_rank),
      asc(ranked.bp),
      desc(ranked.max_combo),
      desc(ranked.server_received_at),
      asc(ranked.course_score_id),
    )
  return limit === undefined ? await query : await query.limit(limit)
}

export type CourseRankingRow = Awaited<ReturnType<typeof fetchCourseRankingRows>>[number]

export interface RankedCourseRow {
  rank: number
  row: CourseRankingRow
}

/**
 * 全 best 行に同点同順位の順位を付け、先頭 limit 件と自分の行を返す。
 * 自分の順位と total は表示範囲外の行も含めて求める。
 */
export function rankCourseRows(rows: CourseRankingRow[], selfId: string | null, limit: number) {
  let previousEx: number | null = null
  let rank = 0
  const ranked: RankedCourseRow[] = rows.map((row, index) => {
    if (previousEx !== row.ex_score) {
      rank = index + 1
      previousEx = row.ex_score
    }
    return { rank, row }
  })
  return {
    entries: ranked.slice(0, limit),
    self: selfId ? ranked.find((entry) => entry.row.player_id === selfId) : undefined,
    total: ranked.length,
  }
}
