import { sql } from 'drizzle-orm'
import type { AnySQLiteColumn } from 'drizzle-orm/sqlite-core'

// Reuse the proposed INSERT value so filling a blank does not add D1 parameters.
export function fillMissingMetadata(column: AnySQLiteColumn) {
  return sql`coalesce(nullif(${column}, ''), nullif(excluded.${sql.identifier(column.name)}, ''), ${column})`
}
