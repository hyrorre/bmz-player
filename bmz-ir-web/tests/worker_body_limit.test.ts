import { expect, mock, test } from 'bun:test'
import { MAX_REPLAY_BYTES } from '../shared/constants/ir'

const fetchHandler = mock(async (request: Request) => new Response(await request.arrayBuffer()))
const scheduled = mock(() => {})
mock.module('nitropack/presets/cloudflare/runtime/cloudflare-module', () => ({
  default: { fetch: fetchHandler, scheduled },
}))
const { default: worker } = await import('../worker')
const run = (request: Request) => worker.fetch(request as never, {} as never, {} as never)

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
    new Request('https://ir.example.invalid/api', {
      method: 'POST',
      body: new Uint8Array(MAX_REPLAY_BYTES),
    }),
  )
  expect((await response.arrayBuffer()).byteLength).toBe(MAX_REPLAY_BYTES)
  expect((await run(new Request('https://ir.example.invalid/api'))).status).toBe(200)
  expect(worker.scheduled).toBe(scheduled)
})
