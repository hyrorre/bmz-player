import { defineEventHandler, getRequestURL } from 'h3'
import { requestBodyLimit } from '../../shared/utils/request_body_limit'
import { readBoundedRequestBody } from '../utils/replay_body'

// Node's development adapter does not pass through worker.ts. Enforce the same
// streaming limit here, before any handler calls H3's unbounded readBody().
export default defineEventHandler(async (event) => {
  const body = await readBoundedRequestBody(
    event,
    requestBodyLimit(event.method, getRequestURL(event).pathname),
  )
  event._requestBody = body
})
