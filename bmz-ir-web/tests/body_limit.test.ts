import { expect, test } from 'bun:test'
import { createServer } from 'node:http'
import { createApp, eventHandler, toNodeListener } from 'h3'
import { readReplayBody } from '../server/utils/replay_body'
import { readLimitedBody, RequestBodyTooLarge } from '../shared/utils/body_limit'
import { MAX_REPLAY_BYTES } from '../shared/constants/ir'

test('an unknown or false length cannot bypass the streaming limit', async () => {
  for (const length of [undefined, '1', 'invalid']) {
    let sent = 0
    let cancelled = false
    const stream = new ReadableStream<Uint8Array>(
      {
        pull(controller) {
          sent++
          if (sent > 16) controller.close()
          else controller.enqueue(new Uint8Array(1024 * 1024))
        },
        cancel() {
          cancelled = true
        },
      },
      { highWaterMark: 0 },
    )
    await expect(readLimitedBody(stream, length, MAX_REPLAY_BYTES)).rejects.toBeInstanceOf(
      RequestBodyTooLarge,
    )
    expect(cancelled).toBe(true)
    expect(sent).toBe(5)
  }
})

test('declared oversized bodies are cancelled without reading', async () => {
  let sent = 0
  let cancelled = false
  const stream = new ReadableStream<Uint8Array>(
    {
      pull() {
        sent++
      },
      cancel() {
        cancelled = true
      },
    },
    { highWaterMark: 0 },
  )
  await expect(
    readLimitedBody(stream, String(MAX_REPLAY_BYTES + 1), MAX_REPLAY_BYTES),
  ).rejects.toBeInstanceOf(RequestBodyTooLarge)
  expect(sent).toBe(0)
  expect(cancelled).toBe(true)
})

test('the exact limit succeeds and empty bodies remain empty', async () => {
  const bytes = new Uint8Array(MAX_REPLAY_BYTES).fill(13)
  expect(await readLimitedBody(new Blob([bytes]).stream(), null, MAX_REPLAY_BYTES)).toEqual(bytes)
  expect(await readLimitedBody(undefined, undefined, MAX_REPLAY_BYTES)).toHaveLength(0)
})

test('the Node HTTP adapter returns 413 for chunked bodies and accepts ordinary bodies', async () => {
  const app = createApp().use(
    eventHandler(async (event) => ({ size: (await readReplayBody(event)).length })),
  )
  const server = createServer(toNodeListener(app))
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const address = server.address() as { port: number }
  try {
    const response = await fetch(`http://127.0.0.1:${address.port}`, {
      method: 'PUT',
      body: new Blob([new Uint8Array(MAX_REPLAY_BYTES + 1)]).stream(),
      duplex: 'half',
    } as RequestInit)
    expect(response.status).toBe(413)
    const small = await fetch(`http://127.0.0.1:${address.port}`, { method: 'PUT', body: 'hello' })
    expect(await small.json()).toEqual({ size: 5 })
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})
