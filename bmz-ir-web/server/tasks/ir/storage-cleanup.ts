import { cleanupIrStorage } from '../../services/storage_cleanup'
import { acquireTaskLock } from '../../services/task_lock'

export default defineTask({
  meta: { name: 'ir:storage-cleanup', description: '期限切れリプレイと失効device keyを清掃する' },
  async run() {
    const lock = await acquireTaskLock('ir:storage-cleanup')
    if (!lock) return { result: { skipped: true, reason: 'already_running' } }
    try {
      return { result: { skipped: false, ...(await cleanupIrStorage()) } }
    } finally {
      await lock.release()
    }
  },
})
