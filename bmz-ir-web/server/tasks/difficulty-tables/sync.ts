import { createError } from 'h3'
import { DIFFICULTY_TABLE_SOURCES } from '../../constants/difficulty_tables'
import {
  difficultyTableSyncFailureDiagnostics,
  syncAllowlistedDifficultyTables,
} from '../../services/difficulty_tables'
import { acquireTaskLock } from '../../services/task_lock'

export default defineTask({
  meta: {
    name: 'difficulty-tables:sync',
    description: 'IR管理のBMS難易度表をD1へ同期する',
  },
  async run() {
    const lock = await acquireTaskLock('difficulty-tables:sync')
    if (!lock) {
      return { result: { skipped: true, reason: 'already_running' } }
    }
    try {
      const result = await syncAllowlistedDifficultyTables(DIFFICULTY_TABLE_SOURCES)
      if (result.failed.length > 0) {
        console.error(
          'difficulty table sync failures',
          difficultyTableSyncFailureDiagnostics(result.failed),
        )
      }
      if (result.successful.length === 0 && result.failed.length > 0) {
        const first = result.failed[0]
        throw createError({
          statusCode: 502,
          statusMessage: 'Difficulty table sync failed',
          message: `all difficulty table syncs failed (${result.failed.length}); first failure: ${first?.sourceUrl ?? 'unknown'}: ${first?.error ?? 'unknown error'}`,
          data: difficultyTableSyncFailureDiagnostics(result.failed),
        })
      }
      return { result: { skipped: false, ...result } }
    } finally {
      await lock.release()
    }
  },
})
