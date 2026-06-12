import { eq } from 'drizzle-orm'
import { db, schema } from 'hub:db'

export async function lookupChartSha256ByMd5(md5: string): Promise<string | null> {
  const rows = await db
    .select({ sha256: schema.charts.sha256 })
    .from(schema.charts)
    .where(eq(schema.charts.md5, md5))
    .limit(1)
  return rows[0]?.sha256 ?? null
}
