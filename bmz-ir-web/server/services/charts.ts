import { and, eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import { publicChartCondition } from './moderation/visibility'

export async function lookupChartSha256ByMd5(md5: string): Promise<string | null> {
  const rows = await db
    .select({ sha256: schema.charts.sha256 })
    .from(schema.charts)
    .where(and(eq(schema.charts.md5, md5), publicChartCondition(schema.charts.sha256)))
    .limit(1)
  return rows[0]?.sha256 ?? null
}
