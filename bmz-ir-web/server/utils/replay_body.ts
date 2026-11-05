import { createError, getHeader, getRequestWebStream, type H3Event } from 'h3'
import { MAX_REPLAY_BYTES } from '../../shared/constants/ir'
import { readLimitedBody, RequestBodyTooLarge } from '../../shared/utils/body_limit'

// H3's Node adapter has neither backpressure nor cancellation. Pause after each
// chunk and close an oversized connection only after its 413 response is sent.
function requestStream(event: H3Event): ReadableStream<Uint8Array> | undefined {
  const req = event.node.req
  if (event.web?.request?.body || event._requestBody || '__unenv__' in req) {
    return getRequestWebStream(event)
  }
  let cleanup = () => {}
  return new ReadableStream<Uint8Array>(
    {
      start(controller) {
        const data = (chunk: Buffer) => {
          req.pause()
          controller.enqueue(chunk)
        }
        const end = () => {
          cleanup()
          controller.close()
        }
        const error = (err: Error) => {
          cleanup()
          controller.error(err)
        }
        cleanup = () => {
          req.off('data', data)
          req.off('end', end)
          req.off('error', error)
        }
        req.on('data', data).once('end', end).once('error', error)
        req.pause()
      },
      pull() {
        req.resume()
      },
      cancel() {
        req.pause()
        cleanup()
        event.node.res.setHeader('Connection', 'close')
        event.node.res.once('finish', () => req.destroy())
      },
    },
    { highWaterMark: 0 },
  )
}

export async function readReplayBody(event: H3Event): Promise<Uint8Array<ArrayBuffer>> {
  try {
    return await readLimitedBody(
      requestStream(event),
      getHeader(event, 'content-length'),
      MAX_REPLAY_BYTES,
    )
  } catch (error) {
    if (error instanceof RequestBodyTooLarge) {
      throw createError({ statusCode: 413, statusMessage: 'Replay body is too large' })
    }
    throw error
  }
}
