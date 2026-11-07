import { afterAll, beforeEach, describe, expect, mock, spyOn, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { eq } from 'drizzle-orm'
import { readdir, readFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { createApp, defineEventHandler, toNodeListener } from 'h3'
import requestBodyMiddleware from '../../middleware/00.request-body'
import * as schema from '../../db/schema'
import type { IrScoreSubmission } from '../../../shared/types/ir'
import type { CourseScoreSubmission } from '../course_ir'
import {
  MAX_METADATA_CHARS,
  MAX_URL_CHARS,
  MAX_SUBARTISTS,
  MAX_COURSE_CHARTS,
  MAX_PLAY_OPTIONS_BYTES,
  MAX_EVIDENCE_BYTES,
} from './input_limits'

// Isolated in-memory SQL database: exercise the real service and atomic score/best batch.
const client = createClient({ url: 'file::memory:' })
const db = drizzle(client, {
  schema,
  logger: {
    logQuery(query, params) {
      if (params.length > 100) {
        throw new RangeError(`D1 bind limit exceeded (${params.length}): ${query}`)
      }
    },
  },
})
mock.module('hub:db', () => ({ db, schema }))
mock.module('../../utils/auth', () => ({ requireIrUser: async () => ({ id: 'player' }) }))
mock.module('../../utils/rate_limit', () => ({
  SCORE_SUBMIT_RATE_LIMIT: {},
  checkUserRateLimit: async () => {},
}))
Object.assign(globalThis, { defineEventHandler })
const { default: scoreHandler } = await import('../../api/v1/scores.post')
const { default: courseHandler } = await import('../../api/v1/course-scores.post')
const { submitScore, prepareBestScoreUpsert } = await import('./submission')
const { validateScoreSubmission } = await import('./common')
const { IrIdempotencyCollisionError } = await import('./idempotency')
const { submitCourseScore, computeCourseHash, validateCourseScoreSubmission } =
  await import('../course_ir')
const { fetchCourseRankingRows, rankCourseRows } = await import('../course_ranking')
const migrations = new URL('../../../../server/db/migrations/sqlite/', import.meta.url)
for (const name of (await readdir(migrations)).filter((name) => name.endsWith('.sql')).sort()) {
  await client.executeMultiple(await readFile(new URL(name, migrations), 'utf8'))
}
await db.insert(schema.users).values([
  { id: 'player', email: 'test@example.invalid', passwordHash: '' },
  { id: 'second', email: 'second@example.invalid', passwordHash: '' },
])
await db.insert(schema.profiles).values([{ id: 'player' }, { id: 'second' }])
const user = { id: 'player', displayName: 'Player' }
const secondUser = { id: 'second' }

afterAll(() => client.close())
beforeEach(async () => {
  await db.delete(schema.bestCourseScores)
  await db.delete(schema.courseScores)
  await db.delete(schema.irCourses)
  await db.delete(schema.bestScores)
  await db.delete(schema.scores)
  await db.delete(schema.charts)
})

function submission(): IrScoreSubmission {
  const counts = { pgreat: 0, great: 0, good: 0, bad: 0, poor: 0, empty_poor: 0 }
  return {
    client: { name: 'bmz', version: '0.3.0', platform: 'windows' },
    chart: { sha256: 'a'.repeat(64) },
    rule: {
      play_mode: 'normal',
      key_mode: '7k',
      gauge: 'normal',
      ln_policy: 'ForceLn',
      effective_ln_mode: 'ln',
      rule_mode: 'Beatoraja',
      judge_algorithm: 'combo',
      scoring: 'bms_ex_score_v1',
    },
    result: {
      clear: 'clear',
      played_at: 1234567890,
      judges: { fast: { ...counts, pgreat: 100 }, slow: counts },
      ex_score: 200,
      max_combo: 100,
      notes: 100,
      min_bp: 0,
      min_cb: 0,
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: 'bmz-score-95',
  }
}

function withNotes(payload: IrScoreSubmission, notes: number) {
  payload.chart.notes = { ...payload.chart.notes, total: notes }
  payload.result.notes = notes
  payload.result.judges.fast.pgreat = notes
  payload.result.ex_score = notes * 2
  payload.result.max_combo = notes
  return payload
}

function courseSubmission() {
  const charts = ['a'.repeat(64)]
  const constraints = { gauge: 'Normal' }
  return {
    client: { name: 'BMZ', version: 'test', platform: 'windows' },
    course: { course_hash: computeCourseHash(charts, constraints), charts, constraints },
    rule: {
      gauge: 'Normal',
      ln_policy: 'AutoLn',
      rule_mode: 'Beatoraja' as const,
      scoring: 'bms_ex_score_v1' as const,
    },
    result: {
      clear: 'Normal',
      course_clear: true,
      course_failed: false,
      played_entries: 1,
      ex_score: 100,
      max_ex_score: 200,
      max_combo: 50,
      bp: 5,
      judges: {},
      gauge_value: 80,
      entries: [{}],
      played_at: 1234567890,
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: 'course-test',
  }
}

function sizedJson(size: number, options = false): Record<string, string> {
  const value: Record<string, string> = options
    ? { device_type: 'keyboard', padding: '' }
    : { padding: '' }
  const remaining = size - Buffer.byteLength(JSON.stringify(value))
  value.padding = 'あ'.repeat(Math.floor(remaining / 3)) + 'x'.repeat(remaining % 3)
  expect(Buffer.byteLength(JSON.stringify(value))).toBe(size)
  return value
}

test('real score and course routes accept exact input limits and return 400 for one more', async () => {
  const app = createApp()
    .use(requestBodyMiddleware)
    .use('/api/v1/scores', scoreHandler)
    .use('/api/v1/course-scores', courseHandler)
  const server = createServer(toNodeListener(app))
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as { port: number }
  let sequence = 0
  const post = (path: string, payload: IrScoreSubmission | CourseScoreSubmission) => {
    payload.idempotency_key = `input-limit-${sequence++}`
    return fetch(`http://127.0.0.1:${port}${path}`, {
      method: 'POST',
      body: JSON.stringify(payload),
      headers: { 'content-type': 'application/json' },
    })
  }
  try {
    const scoreCases: [number, (payload: IrScoreSubmission, count: number) => void][] = []
    for (const field of ['title', 'subtitle', 'artist', 'genre', 'difficulty'] as const) {
      scoreCases.push([
        MAX_METADATA_CHARS,
        (payload, count) => {
          payload.chart[field] = 'あ'.repeat(count)
        },
      ])
    }
    for (const field of ['source', 'append'] as const) {
      scoreCases.push([
        MAX_URL_CHARS,
        (payload, count) => {
          payload.chart.urls = { [field]: 'https://example.invalid/'.padEnd(count, 'x') }
        },
      ])
    }
    scoreCases.push(
      [
        MAX_SUBARTISTS,
        (payload, count) => {
          payload.chart.subartists = Array.from({ length: count }, () => 'artist')
        },
      ],
      [
        MAX_METADATA_CHARS,
        (payload, count) => {
          payload.chart.subartists = ['あ'.repeat(count)]
        },
      ],
      [
        MAX_PLAY_OPTIONS_BYTES,
        (payload, count) => {
          Object.assign(payload, { play_options: sizedJson(count, true) })
        },
      ],
      [
        MAX_EVIDENCE_BYTES,
        (payload, count) => {
          payload.evidence = sizedJson(count)
        },
      ],
    )
    for (const [maximum, set] of scoreCases) {
      for (const extra of [0, 1]) {
        const payload = submission()
        set(payload, maximum + extra)
        const response = await post('/api/v1/scores', payload)
        expect(response.status).toBe(extra ? 400 : 200)
      }
    }
    const courseCases: [number, (payload: CourseScoreSubmission, count: number) => void][] = [
      [
        MAX_METADATA_CHARS,
        (payload, count) => {
          payload.course.title = 'あ'.repeat(count)
        },
      ],
      [
        MAX_URL_CHARS,
        (payload, count) => {
          payload.course.source_url = 'https://example.invalid/'.padEnd(count, 'x')
        },
      ],
      [
        MAX_COURSE_CHARTS,
        (payload, count) => {
          payload.course.charts = Array.from({ length: count }, () => 'a'.repeat(64))
          payload.course.course_hash = computeCourseHash(
            payload.course.charts,
            payload.course.constraints ?? {},
          )
          payload.result.played_entries = count
          payload.result.entries = Array.from({ length: count }, () => ({}))
        },
      ],
      [
        MAX_PLAY_OPTIONS_BYTES,
        (payload, count) => {
          Object.assign(payload, { play_options: sizedJson(count, true) })
        },
      ],
      [
        MAX_EVIDENCE_BYTES,
        (payload, count) => {
          payload.evidence = sizedJson(count)
        },
      ],
    ]
    for (const [maximum, set] of courseCases) {
      for (const extra of [0, 1]) {
        const payload: CourseScoreSubmission = courseSubmission()
        set(payload, maximum + extra)
        const response = await post('/api/v1/course-scores', payload)
        expect(response.status).toBe(extra ? 400 : 200)
      }
    }
    // Current Rust course_payload.rs emits all these keys, including empty
    // choice arrays for each entry. A 4 KiB cap would reject this valid course.
    const payload: CourseScoreSubmission = courseSubmission()
    payload.course.charts = Array.from({ length: MAX_COURSE_CHARTS }, () => 'a'.repeat(64))
    payload.course.course_hash = computeCourseHash(
      payload.course.charts,
      payload.course.constraints ?? {},
    )
    payload.result.played_entries = MAX_COURSE_CHARTS
    payload.result.entries = Array.from({ length: MAX_COURSE_CHARTS }, () => ({}))
    payload.play_options.entry_randomizations = Array.from({ length: MAX_COURSE_CHARTS }, () => ({
      arrange_1p: 'random',
      arrange_2p: 'random',
      seed: '281474976710655',
      seed_scheme: 'beatoraja_24bit_v1',
      bms_random_choices: [],
      bms_switch_choices: [],
    }))
    expect(Buffer.byteLength(JSON.stringify(payload.play_options))).toBeGreaterThan(4096)
    expect((await post('/api/v1/course-scores', payload)).status).toBe(200)
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})

describe('shared submission metadata', () => {
  test('Kaleid paths share the original chart ranking while each best metric retains its source', async () => {
    const first = submission()
    first.rule.ln_policy = 'ForceHln'
    first.rule.effective_ln_mode = 'hln'
    first.chart.features = { hln: true, conditional: true }
    first.chart.ln_profile = { has_defined_hln: true }
    first.chart.notes = { total: 100, ln: 100, hln: 100 }
    first.chart.kaleid = {
      version: 1,
      declared: {},
      source: { hln: true, conditional: true },
      effective: { hln: true },
      diagnostics: [],
    }
    first.play_options.kaleid = { branches: [{ block: 0, branch: 0, time: 1000 }] }
    first.result.min_bp = 10
    first.result.min_cb = 10
    const accepted = await submitScore(user, validateScoreSubmission(first), [], 10)
    const second = structuredClone(first)
    second.idempotency_key += '-second-path'
    second.chart.notes = { total: 50, ln: 50, hln: 50 }
    second.chart.kaleid!.diagnostics = ['partial chart']
    second.play_options.kaleid = { branches: [{ block: 0, branch: 1, time: 1000 }] }
    Object.assign(second.result, {
      notes: 50,
      pass_notes: 50,
      ex_score: 100,
      max_combo: 50,
      min_bp: 0,
      min_cb: 0,
    })
    second.result.judges.fast.pgreat = 50
    const improved = await submitScore(user, validateScoreSubmission(second), [], 10)
    const best = await db.select().from(schema.bestScores)
    expect(best).toHaveLength(1)
    expect(best[0]!.bestExScoreId).toBe(accepted.score_id)
    expect(best[0]!.bestMinBpScoreId).toBe(improved.score_id)
    const histories = await db.select().from(schema.scores)
    expect(histories).toHaveLength(2)
    expect(histories.map((row) => row.playOptions.kaleid)).toEqual([
      first.play_options.kaleid,
      second.play_options.kaleid,
    ])
    const charts = await db.select().from(schema.charts)
    expect(charts).toHaveLength(1)
    expect(charts[0]!.hasConditional).toBe(true)
    expect(charts[0]!.hasHln).toBe(true)
  })
  test('a second player cannot replace a registered chart and numeric conflicts are logged', async () => {
    const first = submission()
    Object.assign(first.chart, {
      title: 'Original title',
      artist: 'Original artist',
      genre: 'Original genre',
      level: 10,
      total: 180,
      judge: 100,
      bpm: { min: 120, max: 180 },
      notes: { total: 100, ln: 1, cn: 2, hcn: 3, mine: 4 },
      features: { random: false, stop: true, ln: true },
      urls: { source: 'https://example.test/original', append: 'https://example.test/append' },
    })
    expect((await submitScore(user, validateScoreSubmission(first), [], 10)).accepted).toBe(true)
    const original = await db.select().from(schema.charts)
    const second = withNotes(structuredClone(first), 120)
    Object.assign(second.chart, {
      title: 'Other title',
      artist: 'Other artist',
      genre: 'Other genre',
      level: 1,
      total: 250,
      judge: 50,
      bpm: { min: 60, max: 240 },
      notes: { total: 120, ln: 9, cn: 8, hcn: 7, mine: 6 },
      features: { random: false, stop: false, ln: false },
      urls: { source: 'https://example.test/other', append: 'https://example.test/other-append' },
    })
    const warning = spyOn(console, 'warn').mockImplementation(() => {})
    try {
      expect(
        (await submitScore(secondUser, validateScoreSubmission(second), [], 10)).accepted,
      ).toBe(true)
      expect(await db.select().from(schema.charts)).toEqual(original)
      expect(warning).toHaveBeenCalledTimes(1)
      expect(warning).toHaveBeenCalledWith('IR chart metadata disagrees with stored values', {
        sha256: first.chart.sha256,
        fields: [
          'level',
          'total',
          'judgeRank',
          'minBpm',
          'maxBpm',
          'notes',
          'lnNotes',
          'cnNotes',
          'hcnNotes',
          'mineNotes',
          'hasStop',
          'hasLn',
        ],
      })
    } finally {
      warning.mockRestore()
    }
  })

  test('a normal submission fills blank and NULL chart fields', async () => {
    const first = submission()
    Object.assign(first.chart, { title: '', artist: 'Artist', genre: '', urls: { source: '' } })
    await submitScore(user, first, [], 10)
    const second = submission()
    Object.assign(second.chart, {
      title: 'Added title',
      artist: 'Other artist',
      subtitle: 'Added subtitle',
      level: 12,
      urls: { source: 'https://example.test/source', append: 'https://example.test/append' },
    })
    await submitScore(secondUser, second, [], 10)
    expect((await db.select().from(schema.charts))[0]).toMatchObject({
      title: 'Added title',
      artist: 'Artist',
      subtitle: 'Added subtitle',
      level: 12,
      genre: '',
      sourceUrl: 'https://example.test/source',
      appendUrl: 'https://example.test/append',
    })
  })

  test.each(['backfill', 'battle'])(
    '%s submissions leave existing chart blanks in place',
    async (source) => {
      await submitScore(user, submission(), [], 10)
      const second = submission()
      second.chart.title = 'A submitted title'
      if (source === 'backfill') second.play_options.submission_source = 'local_backfill'
      else second.play_options.double_option = 'battle'
      await submitScore(secondUser, second, [], 10)
      expect((await db.select().from(schema.charts))[0]?.title).toBe('')
    },
  )

  test('random branches are accepted and true hasRandom survives later false reports', async () => {
    const backfill = withNotes(submission(), 100)
    backfill.chart.features = { random: false }
    backfill.play_options.submission_source = 'local_backfill'
    backfill.play_options.source_kind = 'lr2'
    await submitScore(user, validateScoreSubmission(backfill), [], 10)
    expect((await db.select().from(schema.charts))[0]).toMatchObject({
      notes: 100,
      hasRandom: false,
    })
    const warning = spyOn(console, 'warn').mockImplementation(() => {})
    try {
      for (const [notes, random] of [
        [120, true],
        [130, true],
        [140, false],
      ] as const) {
        const branch = withNotes(submission(), notes)
        branch.chart.features = { random }
        branch.idempotency_key = `branch-${notes}`
        expect(
          (await submitScore(secondUser, validateScoreSubmission(branch), [], 10)).accepted,
        ).toBe(true)
        expect((await db.select().from(schema.charts))[0]).toMatchObject({
          notes: 100,
          hasRandom: true,
        })
      }
      expect(warning).not.toHaveBeenCalled()
      expect(await db.select().from(schema.scores)).toHaveLength(4)
    } finally {
      warning.mockRestore()
    }
  })

  test('a second player cannot replace a registered course title, kind or source URL', async () => {
    const first = courseSubmission()
    const course = {
      ...first.course,
      title: 'Original course',
      kind: 'dan',
      source_url: 'https://example.test/original',
    } as const
    const original = await submitCourseScore(
      user,
      validateCourseScoreSubmission({ ...first, course }),
    )
    expect(original.accepted).toBe(true)
    const stored = await db.select().from(schema.irCourses)
    const second = {
      ...course,
      title: 'Other course',
      kind: 'course',
      source_url: 'https://example.test/other',
    }
    expect(
      (
        await submitCourseScore(
          secondUser,
          validateCourseScoreSubmission({ ...first, course: second }),
        )
      ).accepted,
    ).toBe(true)
    expect(await db.select().from(schema.irCourses)).toEqual(stored)
  })

  test('course submissions fill a blank title and NULL source URL', async () => {
    const payload = courseSubmission()
    await submitCourseScore(user, validateCourseScoreSubmission(payload))
    const course = {
      ...payload.course,
      title: 'Added course',
      source_url: 'https://example.test/course',
    }
    await submitCourseScore(secondUser, validateCourseScoreSubmission({ ...payload, course }))
    expect((await db.select().from(schema.irCourses))[0]).toMatchObject({
      title: course.title,
      sourceUrl: course.source_url,
    })
  })
})

describe('score submission idempotency', () => {
  test('course retries cannot change stored gameplay data or reconstructed bests', async () => {
    const payload = courseSubmission()
    const first = await submitCourseScore(user, payload)
    const stored = await db.select().from(schema.courseScores)
    const best = await db.select().from(schema.bestCourseScores)
    await expect(
      submitCourseScore(user, { ...payload, result: { ...payload.result, ex_score: 200 } }),
    ).rejects.toBeInstanceOf(IrIdempotencyCollisionError)
    expect(await db.select().from(schema.courseScores)).toEqual(stored)
    expect(await db.select().from(schema.bestCourseScores)).toEqual(best)
    // A successful retry may repair a missing best, but only from identical data.
    await db.delete(schema.bestCourseScores)
    const retry = await submitCourseScore(user, payload)
    expect(retry.course_score_id).toBe(first.course_score_id)
    expect((await db.select().from(schema.bestCourseScores))[0]!.exScore).toBe(100)
    const lookup = spyOn(db.query.courseScores, 'findFirst').mockResolvedValueOnce(undefined)
    try {
      await expect(
        submitCourseScore(user, { ...payload, result: { ...payload.result, bp: 0 } }),
      ).rejects.toBeInstanceOf(IrIdempotencyCollisionError)
    } finally {
      lookup.mockRestore()
    }
    for (let i = 0; i < 20; i++) {
      await submitCourseScore(user, {
        ...payload,
        idempotency_key: `repeat-${i}`,
        result: { ...payload.result, ex_score: 200 },
      })
    }
    await submitCourseScore(secondUser, payload)
    const rows = await fetchCourseRankingRows(
      [eq(schema.courseScores.courseHash, payload.course.course_hash)],
      2,
    )
    expect(rows.map((row) => [row.player_id, row.ex_score])).toEqual([
      ['player', 200],
      ['second', 100],
    ])
    // Self rank and total include players outside the returned entries.
    const ranked = rankCourseRows(
      await fetchCourseRankingRows([
        eq(schema.courseScores.courseHash, payload.course.course_hash),
      ]),
      'second',
      1,
    )
    expect(ranked.entries.map(({ rank, row }) => [rank, row.player_id])).toEqual([[1, 'player']])
    expect(ranked.self && [ranked.self.rank, ranked.self.row.player_id]).toEqual([2, 'second'])
    expect(ranked.total).toBe(2)
  })
  test('concurrent prepared best updates preserve independent maxima and their source IDs', async () => {
    const high = submission()
    const low = submission()
    low.idempotency_key = 'lower-score'
    low.result.ex_score = 100
    low.result.clear = 'hard'
    low.rule.gauge = 'hard'
    const first = await submitScore(user, high, [], 10)
    const second = await submitScore(user, low, [], 10)
    await db.delete(schema.bestScores)
    const prepare = (payload: IrScoreSubmission, id: string, rank: number) =>
      prepareBestScoreUpsert(user.id, payload, id, 'unverified', {
        ex_score: payload.result.ex_score,
        clear_rank: rank,
        max_combo: payload.result.max_combo,
        min_bp: payload.result.min_bp,
        min_cb: payload.result.min_cb,
        server_received_at: new Date(),
      })
    // Both requests observe no best row, then the higher score commits first.
    const a = await prepare(high, first.score_id!, 4)
    const b = await prepare(low, second.score_id!, 5)
    await a.statement!
    await b.statement!
    const best = (await db.select().from(schema.bestScores))[0]!
    expect(best.exScore).toBe(200)
    expect(best.scoreId).toBe(first.score_id)
    expect(best.bestExScoreId).toBe(first.score_id)
    expect(best.clearRank).toBe(5)
    expect(best.bestClearScoreId).toBe(second.score_id)
    expect(best.gauge).toBe(high.rule.gauge)
  })
  test.each([false, true])(
    'checks the concurrent insert conflict path (collision=%s)',
    async (collision) => {
      const payload = submission()
      payload.play_options.seed = 123
      const first = await submitScore(user, payload, [], 10)
      const stored = await db.select().from(schema.scores)
      const best = await db.select().from(schema.bestScores)
      // Hide the first lookup to simulate a competing request committing after it.
      const lookup = spyOn(db.query.scores, 'findFirst').mockResolvedValueOnce(undefined)
      const log = spyOn(console, 'warn').mockImplementation(() => {})
      try {
        if (collision) {
          payload.result.ex_score = 201
          await expect(submitScore(user, payload, [], 10)).rejects.toBeInstanceOf(
            IrIdempotencyCollisionError,
          )
        } else {
          expect(await submitScore(user, payload, [], 10)).toMatchObject({
            accepted: true,
            score_id: first.score_id,
          })
        }
        expect(await db.select().from(schema.scores)).toEqual(stored)
        expect(await db.select().from(schema.bestScores)).toEqual(best)
      } finally {
        lookup.mockRestore()
        log.mockRestore()
      }
    },
  )
  test('identical retries return the existing result without inserting or changing scores', async () => {
    const payload = submission()
    payload.play_options.seed = 123
    const first = await submitScore(user, payload, [], 10)
    const stored = await db.select().from(schema.scores)
    const best = await db.select().from(schema.bestScores)
    const retry = await submitScore(
      user,
      {
        ...payload,
        evidence: { obsolete: true },
        play_options: { ...payload.play_options, seed: '123' },
      },
      [],
      10,
    )
    expect(first.accepted).toBe(true)
    expect(retry).toMatchObject({ accepted: true, score_id: first.score_id, best_updated: false })
    expect(await db.select().from(schema.scores)).toEqual(stored)
    expect(await db.select().from(schema.bestScores)).toEqual(best)
  })

  test.each(['chart', 'ex_score', 'rule_mode', 'ln_policy', 'double_option', 'judges'])(
    'rejects a collision in %s without changing either score or best',
    async (field) => {
      const payload = submission()
      const first = await submitScore(user, payload, [], 10)
      const stored = await db.select().from(schema.scores)
      const best = await db.select().from(schema.bestScores)
      const changed = structuredClone(payload)
      if (field === 'chart') changed.chart.sha256 = 'b'.repeat(64)
      if (field === 'ex_score') changed.result.ex_score = 2776
      if (field === 'rule_mode') changed.rule.rule_mode = 'Dx'
      if (field === 'ln_policy') changed.rule.ln_policy = 'ForceCn'
      if (field === 'double_option') changed.play_options.double_option = 'battle'
      if (field === 'judges') changed.result.judges.fast.great = 1
      const log = spyOn(console, 'warn').mockImplementation(() => {})
      try {
        await expect(submitScore(user, changed, [], 10)).rejects.toBeInstanceOf(
          IrIdempotencyCollisionError,
        )
        expect(log).toHaveBeenCalledWith('IR idempotency key collision', {
          playerId: user.id,
          idempotencyKey: payload.idempotency_key,
          existingScoreId: first.score_id,
          existingChartSha256: payload.chart.sha256,
          incomingChartSha256: changed.chart.sha256,
        })
      } finally {
        log.mockRestore()
      }
      expect(await db.select().from(schema.scores)).toEqual(stored)
      expect(await db.select().from(schema.bestScores)).toEqual(best)
      expect((await db.select().from(schema.charts)).length).toBe(1)
    },
  )
})
