import { randomUUID } from 'node:crypto'
import { and, eq, lte } from 'drizzle-orm'
import { db, schema } from 'hub:db'

const DEFAULT_LEASE_MS = 30 * 60 * 1000

export interface AcquiredTaskLock {
  release(): Promise<void>
}

export async function acquireTaskLock(
  name: string,
  leaseMs = DEFAULT_LEASE_MS,
): Promise<AcquiredTaskLock | null> {
  const owner = randomUUID()
  const now = new Date()
  const rows = await db
    .insert(schema.taskLocks)
    .values({ name, owner, leaseUntil: new Date(now.getTime() + leaseMs) })
    .onConflictDoUpdate({
      target: schema.taskLocks.name,
      set: { owner, leaseUntil: new Date(now.getTime() + leaseMs) },
      setWhere: lte(schema.taskLocks.leaseUntil, now),
    })
    .returning({ owner: schema.taskLocks.owner })

  if (rows[0]?.owner !== owner) return null

  return {
    async release() {
      await db
        .delete(schema.taskLocks)
        .where(and(eq(schema.taskLocks.name, name), eq(schema.taskLocks.owner, owner)))
    },
  }
}
