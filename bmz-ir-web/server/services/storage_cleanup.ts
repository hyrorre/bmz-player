import { and, eq, inArray, isNotNull, lt, or } from 'drizzle-orm'
import { blob } from 'hub:blob'
import { db, schema } from 'hub:db'

export const STORAGE_CLEANUP_BATCH_SIZE = 40
export const UNVERIFIED_REPLAY_RETENTION_MS = 24 * 60 * 60 * 1000
export const REVOKED_DEVICE_KEY_RETENTION_MS = 90 * 24 * 60 * 60 * 1000

interface RejectedReplay {
  id: string
  objectPath: string
}

/** Paths are unique per upload/PUT. Never clear a newer generation's reference. */
export async function deleteRejectedReplayBlobs(rows: RejectedReplay[]): Promise<number> {
  if (rows.length === 0) return 0
  // Two binds per row plus fixed update/status binds, below D1's 100 limit.
  if (rows.length > STORAGE_CLEANUP_BATCH_SIZE) throw new Error('replay cleanup batch is too large')
  const results = await Promise.allSettled(rows.map((row) => blob.del(row.objectPath)))
  const deleted = rows.filter((_, index) => results[index]?.status === 'fulfilled')
  const failed = results.find((result) => result.status === 'rejected')
  if (failed?.status === 'rejected') {
    console.warn('rejected replay blob cleanup failed', failed.reason)
  }
  if (deleted.length === 0) return 0
  try {
    await db
      .update(schema.replayObjects)
      .set({ objectPath: null })
      .where(
        and(
          eq(schema.replayObjects.status, 'rejected'),
          or(
            ...deleted.map((row) =>
              and(
                eq(schema.replayObjects.id, row.id),
                eq(schema.replayObjects.objectPath, row.objectPath),
              ),
            ),
          ),
        ),
      )
    return deleted.length
  } catch (error) {
    // Keep references for the next scheduled run if storage/DB is unavailable.
    console.warn('rejected replay blob cleanup failed', error)
    return 0
  }
}

export async function cleanupIrStorage(now = new Date()) {
  // The subquery and status change are one SQL statement. A fresh upload,
  // verification or renewed intent wins if it commits before this claim.
  const expired = db
    .select({ id: schema.replayObjects.id })
    .from(schema.replayObjects)
    .where(
      and(
        inArray(schema.replayObjects.status, ['pending_upload', 'uploaded']),
        lt(
          schema.replayObjects.updatedAt,
          new Date(now.getTime() - UNVERIFIED_REPLAY_RETENTION_MS),
        ),
      ),
    )
    .orderBy(schema.replayObjects.updatedAt, schema.replayObjects.id)
    .limit(STORAGE_CLEANUP_BATCH_SIZE)
  const claimed = await db
    .update(schema.replayObjects)
    .set({ status: 'rejected', updatedAt: now })
    .where(inArray(schema.replayObjects.id, expired))
    .returning({ id: schema.replayObjects.id })

  // Include previously rejected blobs so transient deletion failures are retried.
  const rejected = await db
    .select({ id: schema.replayObjects.id, objectPath: schema.replayObjects.objectPath })
    .from(schema.replayObjects)
    .where(
      and(eq(schema.replayObjects.status, 'rejected'), isNotNull(schema.replayObjects.objectPath)),
    )
    .orderBy(schema.replayObjects.updatedAt, schema.replayObjects.id)
    .limit(STORAGE_CLEANUP_BATCH_SIZE)
  const deletedBlobs = await deleteRejectedReplayBlobs(
    rejected.filter((row): row is RejectedReplay => row.objectPath !== null),
  )

  // Historical score verification is persisted; device keys are only looked up
  // for new evidence, where revoked keys are already refused. No score FK targets them.
  const oldKeys = db
    .select({ id: schema.deviceKeys.id })
    .from(schema.deviceKeys)
    .where(
      lt(schema.deviceKeys.revokedAt, new Date(now.getTime() - REVOKED_DEVICE_KEY_RETENTION_MS)),
    )
    .orderBy(schema.deviceKeys.revokedAt, schema.deviceKeys.id)
    .limit(STORAGE_CLEANUP_BATCH_SIZE)
  const deletedKeys = await db
    .delete(schema.deviceKeys)
    .where(inArray(schema.deviceKeys.id, oldKeys))
    .returning({ id: schema.deviceKeys.id })
  return {
    expired_replays: claimed.length,
    deleted_replay_blobs: deletedBlobs,
    deleted_device_keys: deletedKeys.length,
  }
}
