/** Enforce limits while consuming the stream, including requests without a length header. */
export class RequestBodyTooLarge extends Error {}

export async function readLimitedBody(
  stream: ReadableStream<Uint8Array> | null | undefined,
  contentLength: string | null | undefined,
  maxBytes: number,
): Promise<Uint8Array<ArrayBuffer>> {
  const reader = stream?.getReader()
  // Keep one bounded buffer: an array of one-byte chunks can itself consume
  // far more memory than the payload limit, even when the bytes fit.
  let body = new Uint8Array(0)
  let size = 0
  try {
    if (Number(contentLength) > maxBytes) throw new RequestBodyTooLarge('Request body is too large')
    if (reader) {
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        if (value.byteLength > maxBytes - size)
          throw new RequestBodyTooLarge('Request body is too large')
        const nextSize = size + value.byteLength
        if (nextSize > body.length) {
          const grown = new Uint8Array(
            Math.min(maxBytes, Math.max(nextSize, 4096, body.length * 2)),
          )
          grown.set(body.subarray(0, size))
          body = grown
        }
        body.set(value, size)
        size = nextSize
      }
    }
    return body.subarray(0, size)
  } catch (error) {
    // Do not wait for cancellation: an untrusted source may never settle it.
    void reader?.cancel(error).catch(() => {})
    throw error
  } finally {
    reader?.releaseLock()
  }
}
