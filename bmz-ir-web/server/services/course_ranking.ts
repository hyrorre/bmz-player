import { and, sql, type SQL } from 'drizzle-orm'
import { db, schema } from 'hub:db'

/** Course BP includes empty POORs and unplayed notes. CB excludes only empty POORs. */
export function courseComboBreaks(bp: number, judges?: Record<string, unknown>) {
  const emptyPoor = judges?.empty_poor
  return typeof emptyPoor === 'number' &&
    Number.isSafeInteger(emptyPoor) &&
    emptyPoor >= 0 &&
    emptyPoor <= bp
    ? bp - emptyPoor
    : null
}

type StoredCourseScore = typeof schema.courseScores.$inferSelect
interface CourseRankingRow {
  player_id: string
  course_score_id: string
  ex_score: number
  max_ex_score: number
  clear_type: string
  course_clear: boolean
  max_combo: number
  bp: number
  device_type: StoredCourseScore['deviceType']
  ln_policy: string
  rule_mode: StoredCourseScore['ruleMode']
  played_at: Date | null
  verification: StoredCourseScore['verification']
}

export interface RankedCourseRow {
  rank: number
  row: CourseRankingRow
}

export async function fetchCourseRankingPage(
  conditions: SQL[],
  selfId: string | null,
  limit: number,
  offset = 0,
) {
  type Raw = Omit<CourseRankingRow, 'played_at' | 'course_clear'> & {
    played_at: number | null
    course_clear: number
    position: number
    rank: number
    total: number
  }
  const order = sql`ex_score desc, clear_rank desc, bp, max_combo desc, server_received_at desc, id`
  const rows = await db.all<Raw>(sql`
    with candidates as (
      select id, player_id, ex_score, clear_rank, bp, max_combo, server_received_at,
        row_number() over(partition by player_id order by ${order}) as personal
      from ${schema.courseScores} where ${and(...conditions)}
    ), ranked as (
      select id, player_id, rank() over(order by ex_score desc) as rank,
        row_number() over(order by ${order}) as position
      from candidates where personal=1
    ), page as (
      select * from ranked where (position>${offset} and position<=${offset + limit}) or player_id=${selfId}
    )
    select totals.total, p.rank, p.position, s.id as course_score_id, s.player_id, s.ex_score, s.max_ex_score,
      s.clear_type, s.course_clear, s.max_combo, s.bp, s.device_type, s.ln_policy, s.rule_mode, s.played_at, s.verification
    from (select count(*) as total from ranked) totals left join page p on true
    left join ${schema.courseScores} s on s.id=p.id order by p.position
  `)
  const entries: RankedCourseRow[] = []
  let self: RankedCourseRow | undefined
  for (const row of rows) {
    if (!row.course_score_id) continue
    const entry = {
      rank: row.rank,
      row: {
        ...row,
        course_clear: !!row.course_clear,
        played_at: row.played_at === null ? null : new Date(row.played_at),
      },
    }
    if (row.player_id === selfId) self = entry
    if (row.position > offset && row.position <= offset + limit) entries.push(entry)
  }
  return { entries, self, total: rows[0]!.total }
}
