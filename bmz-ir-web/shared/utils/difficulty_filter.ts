import type { IrDifficultyLabel } from '../types/ir'

export interface DifficultyFilter {
  difficulty_table: string
  difficulty_level?: string
}

export function readDifficultyFilter(query: Record<string, unknown>): DifficultyFilter | undefined {
  const { difficulty_table, difficulty_level } = query
  if (typeof difficulty_table !== 'string' || !difficulty_table) return
  if (typeof difficulty_level === 'string') return { difficulty_table, difficulty_level }
  if (difficulty_level === undefined) return { difficulty_table }
}

export function difficultyFilterForLabel(label: IrDifficultyLabel): DifficultyFilter {
  return { difficulty_table: label.table_id, difficulty_level: label.level }
}
