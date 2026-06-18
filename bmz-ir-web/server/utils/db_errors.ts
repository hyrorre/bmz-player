export function isUniqueConstraintError(error: unknown): boolean {
  return hasConstraintMessage(error, new Set())
}

function hasConstraintMessage(error: unknown, seen: Set<unknown>): boolean {
  if (!(error instanceof Error) || seen.has(error)) {
    return false
  }
  seen.add(error)

  if (/unique constraint|constraint failed|SQLITE_CONSTRAINT/i.test(error.message)) {
    return true
  }

  const cause = (error as Error & { cause?: unknown }).cause
  return hasConstraintMessage(cause, seen)
}
