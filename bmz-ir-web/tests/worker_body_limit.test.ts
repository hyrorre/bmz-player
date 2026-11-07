import { beforeEach, expect, mock, test } from 'bun:test'
import { MAX_REPLAY_BYTES } from '../shared/constants/ir'
import { MAX_REQUEST_BODY_BYTES } from '../shared/utils/request_body_limit'

const fetchHandler = mock(async (request: Request) => new Response(await request.arrayBuffer()))
const scheduled = mock(() => {})
mock.module('nitropack/presets/cloudflare/runtime/cloudflare-module', () => ({
  default: { fetch: fetchHandler, scheduled },
}))
const { default: worker } = await import('../worker')
const run = (request: Request) => worker.fetch(request as never, {} as never, {} as never)
beforeEach(() => fetchHandler.mockClear())

test('the Worker rejects an oversized stream before Nitro buffers it', async () => {
  let sent = 0
  let cancelled = false
  const body = new ReadableStream<Uint8Array>(
    {
      pull(controller) {
        sent++
        controller.enqueue(new Uint8Array(1024 * 1024))
      },
      cancel() {
        cancelled = true
      },
    },
    { highWaterMark: 0 },
  )
  const response = await run(
    new Request('https://ir.example.invalid/api/v1/scores/id/replay/upload', {
      method: 'PUT',
      body,
      duplex: 'half',
    } as RequestInit),
  )
  expect(response.status).toBe(413)
  expect(cancelled).toBe(true)
  expect(sent).toBe(5)
  expect(fetchHandler).not.toHaveBeenCalled()
})

test('the Worker delegates bounded bodies, GETs and scheduled events', async () => {
  const response = await run(
    new Request('https://ir.example.invalid/api/v1/scores/id/replay/upload', {
      method: 'PUT',
      body: new Uint8Array(MAX_REPLAY_BYTES),
    }),
  )
  expect((await response.arrayBuffer()).byteLength).toBe(MAX_REPLAY_BYTES)
  expect((await run(new Request('https://ir.example.invalid/api'))).status).toBe(200)
  expect(worker.scheduled).toBe(scheduled)
})

test('ordinary bodies accept exactly 256 KiB and reject one more byte on every other route', async () => {
  for (const [method, path] of [
    ['POST', '/api/v1/scores'],
    ['POST', '/api/v1/course-scores'],
    ['POST', '/api/v1/scores/id/replay/upload-url'],
    ['POST', '/api/v1/scores/id/replay/upload'],
    ['PUT', '/api/v1/scores/id/replay/upload/extra'],
  ]) {
    const url = `https://ir.example.invalid${path}`
    expect(
      (await run(new Request(url, { method, body: new Uint8Array(MAX_REQUEST_BODY_BYTES) })))
        .status,
    ).toBe(200)
    const calls = fetchHandler.mock.calls.length
    expect(
      (await run(new Request(url, { method, body: new Uint8Array(MAX_REQUEST_BODY_BYTES + 1) })))
        .status,
    ).toBe(413)
    expect(fetchHandler.mock.calls).toHaveLength(calls)
  }
})
