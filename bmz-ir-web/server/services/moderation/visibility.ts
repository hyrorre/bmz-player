import { sql, type SQL } from 'drizzle-orm'
import type { AnySQLiteColumn } from 'drizzle-orm/sqlite-core'
import { schema } from 'hub:db'
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
