import { and, eq, sql, type SQL } from 'drizzle-orm'
import type { AnySQLiteColumn } from 'drizzle-orm/sqlite-core'
import { db, schema } from 'hub:db'
import { NON_PUBLIC_CHART_STATUSES } from '../../../shared/constants/moderation'

/**
 * 公開読み取りに使う共通条件。
 *
 * BAN・譜面ブロックは既存の scores / best_scores を書き換えず、読み取り時に除外する。
 * そのため解除すると再計算なしで元の公開状態に戻る。`scores.accepted` はスコア自体の
 * 受理判定のまま残し、ここでは扱わない。
 *
 * 条件は相関サブクエリで、WHERE に入れてから limit / offset / count を適用する。
 * 取得後に配列から取り除くと順位・件数・ページングが欠けるため、そうしないこと。
 */

const nonPublicChartStatuses = sql.join(
  NON_PUBLIC_CHART_STATUSES.map((status) => sql`${status}`),
  sql`, `,
)

/** プレイヤーが BAN されていないこと。submit_suspended は公開を維持する。 */
export function publicPlayerCondition(playerId: AnySQLiteColumn | SQL): SQL {
  const moderation = schema.userModeration
  return sql`not exists (select 1 from ${moderation} where ${moderation.userId} = ${playerId} and ${moderation.status} = 'banned')`
}

/** 譜面が hidden / blocked でないこと。active / under_review は公開する。 */
export function publicChartCondition(chartSha256: AnySQLiteColumn | SQL): SQL {
  const moderation = schema.chartModeration
  return sql`not exists (select 1 from ${moderation} where ${moderation.chartSha256} = ${chartSha256} and ${moderation.status} in (${nonPublicChartStatuses}))`
}

/**
 * コースを構成する譜面がすべて公開状態であること。
 *
 * course_hash は譜面 sha256 リストから計算され、送信時に一致を検証している。
 * 保存済みの ir_courses.charts を json_each で展開して判定し、コース hash だけでは
 * 回避できないようにする。
 */
export function publicCourseCondition(courseHash: AnySQLiteColumn | SQL): SQL {
  const courses = schema.irCourses
  const moderation = schema.chartModeration
  return sql`not exists (select 1 from ${courses}, json_each(${courses.charts}) as course_chart
    inner join ${moderation} on ${moderation.chartSha256} = course_chart.value
    where ${courses.courseHash} = ${courseHash} and ${moderation.status} in (${nonPublicChartStatuses}))`
}

export async function isPublicPlayer(playerId: string): Promise<boolean> {
  const rows = await db
    .select({ id: schema.profiles.id })
    .from(schema.profiles)
    .where(and(eq(schema.profiles.id, playerId), publicPlayerCondition(schema.profiles.id)))
    .limit(1)
  return rows.length > 0
}

/** 譜面が公開状態か。charts に未登録でも hidden / blocked でなければ true。 */
export async function isPublicChart(chartSha256: string): Promise<boolean> {
  const rows = await db
    .select({ status: schema.chartModeration.status })
    .from(schema.chartModeration)
    .where(eq(schema.chartModeration.chartSha256, chartSha256))
    .limit(1)
  const status = rows[0]?.status
  return !status || !(NON_PUBLIC_CHART_STATUSES as readonly string[]).includes(status)
}

/** 登録済みコースの構成譜面がすべて公開状態か。未登録のコースは false。 */
export async function isPublicCourse(courseHash: string): Promise<boolean> {
  const rows = await db
    .select({ hash: schema.irCourses.courseHash })
    .from(schema.irCourses)
    .where(
      and(
        eq(schema.irCourses.courseHash, courseHash),
        publicCourseCondition(schema.irCourses.courseHash),
      ),
    )
    .limit(1)
  return rows.length > 0
}

/**
 * スコア詳細・リプレイの公開可否。BAN済みプレイヤーや非公開譜面のスコアは、
 * 本人以外には存在しないものとして扱う。BAN済みの本人は認証できないため閲覧できない。
 */
export async function isScoreVisibleTo(
  score: { playerId: string; chartSha256: string },
  viewerId: string | null,
): Promise<boolean> {
  if (viewerId === score.playerId) {
    return true
  }
  return (await isPublicPlayer(score.playerId)) && (await isPublicChart(score.chartSha256))
}
