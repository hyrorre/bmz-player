import { DIFFICULTY_TABLE_SOURCES } from '../../constants/difficulty_tables'
import { syncAllowlistedDifficultyTables } from '../../services/difficulty_tables'

export default defineTask({
  meta: {
    name: 'difficulty-tables:sync',
    description: 'IR管理のBMS難易度表をD1へ同期する',
  },
  async run() {
    const result = await syncAllowlistedDifficultyTables(DIFFICULTY_TABLE_SOURCES)
    if (result.successful.length === 0 && result.failed.length > 0) {
      throw new Error(`all difficulty table syncs failed (${result.failed.length})`)
    }
    return { result }
  },
})
