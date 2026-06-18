import { getHeader, type H3Event } from 'h3'

export const DEFAULT_IR_PROVIDER_KEY = 'bmz'
export const LOCAL_IR_PROVIDER_KEY = 'bmz-dev'

export function irProviderKeyForEvent(event: H3Event): string {
  const configured = normalizeProviderKey(useRuntimeConfig(event).ir?.providerKey)
  if (configured) {
    return configured
  }
  return irProviderKeyForHost(getHeader(event, 'x-forwarded-host') ?? getHeader(event, 'host'))
}

export function irProviderKeyForHost(host: string | undefined | null): string {
  if (!host) {
    return DEFAULT_IR_PROVIDER_KEY
  }
  const normalized = host.trim().toLowerCase()
  if (
    normalized.startsWith('localhost') ||
    normalized.startsWith('127.0.0.1') ||
    normalized.startsWith('0.0.0.0') ||
    normalized.startsWith('[::1]')
  ) {
    return LOCAL_IR_PROVIDER_KEY
  }
  return DEFAULT_IR_PROVIDER_KEY
}

function normalizeProviderKey(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}
