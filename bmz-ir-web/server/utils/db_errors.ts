export function isUniqueConstraintError(error: unknown): boolean {
  return hasConstraintMessage(error, /unique constraint failed|SQLITE_CONSTRAINT_UNIQUE/i)
}

export function isScoreHistoryRevisionMismatch(error: unknown): boolean {
  return hasConstraintMessage(
    error,
    /NOT NULL constraint failed: profiles\.score_history_revision/i,
  )
}

function hasConstraintMessage(error: unknown, pattern: RegExp): boolean {
  const seen = new Set<Error>()
  while (error instanceof Error && !seen.has(error)) {
    if (pattern.test(error.message)) return true
    seen.add(error)
    error = error.cause
  }
  return false
}
