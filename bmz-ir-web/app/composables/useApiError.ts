const apiErrorKeys: Record<string, string> = {
  'Authentication required': 'apiErrors.authenticationRequired',
  'Invalid credentials': 'apiErrors.invalidCredentials',
  'Invalid current password': 'apiErrors.invalidCurrentPassword',
  'Account already exists': 'apiErrors.accountAlreadyExists',
  'email is already registered': 'apiErrors.emailAlreadyRegistered',
  'Profile not found': 'apiErrors.profileNotFound',
  'Player not found': 'apiErrors.playerNotFound',
  'Chart not found': 'apiErrors.chartNotFound',
  'Course not found': 'apiErrors.courseNotFound',
  'Score not found': 'apiErrors.scoreNotFound',
  'Replay is not available': 'apiErrors.replayNotAvailable',
  'Session not found': 'apiErrors.sessionNotFound',
  'Device key not found or already revoked': 'apiErrors.deviceKeyNotFound',
  'Score submission is suspended for this account': 'apiErrors.accountSuspended',
  'This account is banned from BMZ IR': 'apiErrors.accountBanned',
  'Score submission is temporarily disabled for this chart': 'apiErrors.chartHidden',
  'Score submission is blocked for this chart': 'apiErrors.chartBlocked',
  'Administrator access required': 'apiErrors.administratorRequired',
  'Moderation status was changed by another operation': 'apiErrors.moderationConflict',
  'Administrators cannot change their own account status': 'apiErrors.cannotChangeOwnStatus',
  'Administrator accounts cannot be restricted': 'apiErrors.cannotRestrictAdministrator',
}

function extractApiMessage(error: unknown): string | undefined {
  if (!error || typeof error !== 'object') return undefined

  const candidate = error as {
    message?: unknown
    statusMessage?: unknown
    data?: { message?: unknown; statusMessage?: unknown }
  }
  for (const value of [
    candidate.data?.statusMessage,
    candidate.data?.message,
    candidate.statusMessage,
  ]) {
    if (typeof value === 'string' && value) return value
  }

  if (typeof candidate.message === 'string') {
    const match = /\]\s*:\s*(.+)$/u.exec(candidate.message)
    return match?.[1] ?? candidate.message
  }
}

export function useApiError() {
  const { t } = useI18n()

  function translateApiError(error: unknown, fallbackKey: string) {
    const message = extractApiMessage(error)
    const key = message ? apiErrorKeys[message] : undefined
    return key ? t(key) : t(fallbackKey)
  }

  return { translateApiError }
}
