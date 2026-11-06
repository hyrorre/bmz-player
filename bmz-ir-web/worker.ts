import nitroWorker from 'nitropack/presets/cloudflare/runtime/cloudflare-module'
import { MAX_REPLAY_BYTES } from './shared/constants/ir'
import { readLimitedBody, RequestBodyTooLarge } from './shared/utils/body_limit'

// Nitro's Cloudflare adapter buffers the body before H3 handlers run. Bound it
// at the Worker entry as well, preserving the preset's scheduled/event handlers.
type WorkerFetch = NonNullable<typeof nitroWorker.fetch>
export default {
  ...nitroWorker,
  async fetch(...args: Parameters<WorkerFetch>): Promise<Awaited<ReturnType<WorkerFetch>>> {
    const [request, env, context] = args
    if (!request.body) return nitroWorker.fetch!(request, env, context)
    try {
      const body = await readLimitedBody(
        request.body as unknown as ReadableStream<Uint8Array>,
        request.headers.get('content-length'),
        MAX_REPLAY_BYTES,
      )
      const bounded = new Request(request as unknown as Request, { body })
      bounded.headers.delete('transfer-encoding')
      bounded.headers.set('content-length', String(body.byteLength))
      return nitroWorker.fetch!(bounded as unknown as typeof request, env, context)
    } catch (error) {
      if (error instanceof RequestBodyTooLarge) {
        return new Response('Request body is too large', { status: 413 }) as unknown as Awaited<
          ReturnType<WorkerFetch>
        >
      }
      throw error
    }
  },
}
