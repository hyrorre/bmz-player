import { defineEventHandler } from 'h3'
import { listDifficultyTableOptions } from '../../services/difficulty_tables'

export default defineEventHandler(async () => ({ tables: await listDifficultyTableOptions() }))
