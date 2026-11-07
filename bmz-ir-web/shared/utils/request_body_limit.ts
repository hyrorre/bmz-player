import { MAX_REPLAY_BYTES } from '../constants/ir'

export const MAX_REQUEST_BODY_BYTES = 256 * 1024

export function requestBodyLimit(method: string, pathname: string): number {
  return method === 'PUT' && /^\/api\/v1\/scores\/[^/]+\/replay\/upload\/?$/.test(pathname)
    ? MAX_REPLAY_BYTES
    : MAX_REQUEST_BODY_BYTES
}
