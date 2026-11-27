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
const { deleteLocalBackfillScores, submitScore } = await import('./submission')
const { parseRankingQuery, validateScoreSubmission } = await import('./common')
const { getRanking, getRankingWithPreviousRank } = await import('./ranking')
const { IrIdempotencyCollisionError } = await import('./idempotency')
const { submitCourseScore, computeCourseHash, validateCourseScoreSubmission } =
  await import('../course_ir')
const { courseComboBreaks, fetchCourseRankingPage } = await import('../course_ranking')
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

test('joins details only for a full ranking page and the player outside that page', async () => {
  const playerIds = Array.from({ length: 202 }, (_, i) => `ranking-player-${i}`)
  await client.execute(`
    WITH RECURSIVE players(i) AS (VALUES(0) UNION ALL SELECT i + 1 FROM players WHERE i < 201)
    INSERT INTO users (id, email, password_hash)
    SELECT 'ranking-player-' || i, 'ranking-' || i || '@example.invalid', '' FROM players
  `)
  await client.execute(`
    INSERT INTO profiles (id, display_name)
    SELECT id, 'Name ' || id FROM users WHERE id LIKE 'ranking-player-%'
  `)
  for (const [i, id] of playerIds.entries()) {
    const payload = submission()
    payload.result.played_at = Number(payload.result.played_at) + i
    if (i >= 101) payload.result.clear = 'Failed'
    await submitScore({ id }, validateScoreSubmission(payload), [], 10)
  }
  const own = submission()
  own.result.clear = 'Failed'
  own.result.ex_score = 100
  own.result.judges.fast.pgreat = 0
  own.result.judges.fast.great = 100
  own.play_options.arrange_1p = 'mirror'
  const { score_id } = await submitScore(user, validateScoreSubmission(own), [], 10)
  const execute = spyOn(client, 'execute')
  let result
  try {
    result = await getRankingWithPreviousRank(
      user,
      own.chart.sha256,
      parseRankingQuery({ limit: 200, offset: 2 }),
      199,
    )
    const detailQueries = execute.mock.calls.flatMap(([statement]) =>
      typeof statement === 'object' && statement.sql.includes('as ranking_sources')
        ? [statement]
        : [],
    )
    expect(detailQueries).toHaveLength(1)
    const binds = detailQueries[0]!.args
    expect(binds).toHaveLength(1)
    const sources = JSON.parse(String((binds as unknown[])[0]))
    expect(sources.map((source: { playerId: string }) => source.playerId)).toEqual([
      ...playerIds.slice(2),
      user.id,
    ])
  } finally {
    execute.mockRestore()
  }

  const { ranking } = result.data
  expect(result.previousRank).toBe(203)
  expect(ranking.clear_rate).toBe(50)
  expect(ranking.pagination).toEqual({ limit: 200, offset: 2, total: 203, has_more: true })
  expect(ranking.entries).toHaveLength(200)
  expect(ranking.entries[0]).toMatchObject({
    rank: 1,
    scope_rank: 3,
    player: { id: playerIds[2], display_name: `Name ${playerIds[2]}` },
  })
  expect(ranking.self).toMatchObject({
    rank: 203,
    score_id,
    included_in_entries: false,
    entry: { scope_rank: 203, score: { arrange_1p: 'mirror', judges: own.result.judges } },
  })
  await db.insert(schema.rivalRelationships).values({
    ownerPlayerId: user.id,
    targetPlayerId: playerIds[0]!,
    relationType: 'rival',
  })
  for (const [scope, total] of [
    ['self', 1],
    ['rivals', 1],
    ['self_and_rivals', 2],
    ['around_self', 6],
  ] as const) {
    const { ranking } = await getRanking(
      user,
      own.chart.sha256,
      parseRankingQuery({ scope, limit: 1 }),
    )
    expect(ranking.pagination.total).toBe(total)
    expect(ranking.clear_rate).toBe(50)
    expect(ranking.self?.rank).toBe(203)
    expect(ranking.entries[0]?.rank).toBe(scope === 'self' ? 203 : 1)
    expect(ranking.entries[0]?.scope_rank).toBe(1)
  }
  const beyond = await getRankingWithPreviousRank(
    user,
    own.chart.sha256,
    parseRankingQuery({ offset: 300 }),
    200,
  )
  expect(beyond.previousRank).toBe(1)
  expect(beyond.data.ranking.entries).toEqual([])
  expect(beyond.data.ranking.pagination.total).toBe(203)
  expect(beyond.data.ranking.self?.included_in_entries).toBe(false)
})

test('keeps independent bests and their details when falling back to history', async () => {
  const bestEx = submission()
  bestEx.result.max_combo = 80
  bestEx.play_options.arrange_1p = 'random'
  const ex = await submitScore(user, validateScoreSubmission(bestEx), [], 10)
  const bestClear = submission()
  bestClear.idempotency_key = 'best-clear'
  bestClear.rule.ln_policy = 'ForceCn'
  bestClear.rule.effective_ln_mode = 'cn'
  bestClear.result.clear = 'Hard'
  bestClear.result.ex_score = 100
  bestClear.result.max_combo = 90
  bestClear.result.notes = 110
  bestClear.result.judges.fast.pgreat = 0
  bestClear.result.judges.fast.great = 100
  bestClear.play_options.arrange_1p = 'mirror'
  const clear = await submitScore(user, validateScoreSubmission(bestClear), [], 10)
  const bestCombo = structuredClone(bestClear)
  bestCombo.idempotency_key = 'best-combo'
  bestCombo.result.clear = 'Normal'
  bestCombo.result.max_combo = 100
  bestCombo.result.notes = 120
  const combo = await submitScore(user, validateScoreSubmission(bestCombo), [], 10)
  const query = parseRankingQuery({ scope: 'around_self', limit: 1 })
  const cached = await getRanking(user, bestEx.chart.sha256, query)
  expect(cached.ranking.entries[0]).toMatchObject({
    rank: 1,
    score: {
      score_id: ex.score_id,
      clear: 'Hard',
      max_ex_score: 200,
      max_combo: 100,
      max_combo_notes: 120,
      arrange_1p: 'random',
      judges: bestEx.result.judges,
      source_score_ids: { ex_score: ex.score_id, clear: clear.score_id, max_combo: combo.score_id },
    },
  })
  expect(cached.ranking.self?.included_in_entries).toBe(true)
  await db.delete(schema.bestScores)
  expect(await getRanking(user, bestEx.chart.sha256, query)).toEqual(cached)
})

test.skipIf(!process.env.BMZ_RANKING_BENCH)('measure ranking page transfers', async () => {
  const chart = submission()
  const course = courseSubmission()
  await submitScore(user, validateScoreSubmission(chart), [], 10)
  await submitCourseScore(user, course)
  await client.execute('create temp table bench_players(i integer primary key)')
  await client.execute(
    'with recursive n(i) as (values (1) union all select i+1 from n where i<5000) insert into bench_players select i from n',
  )
  await client.execute(
    "insert into users(id,email,password_hash) select 'bench-'||i, i||'@bench.invalid','' from bench_players",
  )
  await client.execute("insert into profiles(id) select 'bench-'||i from bench_players")
  for (const table of ['scores', 'best_scores', 'course_scores']) {
    const columns = (await client.execute('pragma table_info(' + table + ')')).rows.map((row) =>
      String(row.name),
    )
    const values = columns.map((c) => {
      if (c === 'player_id') return "'bench-' || i"
      if (c === 'id' || c === 'score_id' || (c.startsWith('best_') && c.endsWith('_id')))
        return "'bench-score-' || i"
      if (c === 'ex_score') return 'i / 2'
      return 's.' + c
    })
    await client.execute(
      'insert into ' +
        table +
        ' (' +
        columns.join(',') +
        ') select ' +
        values.join(',') +
        ' from ' +
        table +
        " s cross join bench_players where s.player_id='player'",
    )
  }
  const measure = async (name: string, query: () => Promise<unknown>) => {
    const run = client.execute.bind(client)
    let transferred = 0
    const spy = spyOn(client, 'execute').mockImplementation(async (...args) => {
      const result = await run(...args)
      transferred += result.rows.length
      return result
    })
    try {
      const times = []
      for (let i = 0; i < 9; i++) {
        transferred = 0
        const start = performance.now()
        await query()
        times.push(performance.now() - start)
      }
      console.log(name, 'rows', transferred, 'median ms', times.sort((a, b) => a - b)[4])
    } finally {
      spy.mockRestore()
    }
  }
  await measure('chart', () =>
    getRanking(user, chart.chart.sha256, parseRankingQuery({ limit: 20 })),
  )
  await measure('course', () =>
    fetchCourseRankingPage(
      [eq(schema.courseScores.courseHash, course.course.course_hash)],
      user.id,
      20,
    ),
  )
})

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

test('course ranking reads recorded arrangements and accepts retries of legacy scores', async () => {
  const payload: CourseScoreSubmission = courseSubmission()
  payload.play_options = {
    device_type: 'keyboard',
    option: 'normal',
    entry_randomizations: [{ arrange_1p: 'random', arrange_2p: 'mirror', seed: '5' }],
  }
  const submitted = await submitCourseScore(user, validateCourseScoreSubmission(payload))
  const conditions = [eq(schema.courseScores.courseHash, payload.course.course_hash)]
  expect((await fetchCourseRankingPage(conditions, null, 1)).entries[0]?.row).toMatchObject({
    max_ex_score: 200,
    ln_policy: 'AutoLn',
  })
  expect((await db.select().from(schema.courseScores))[0]?.arrangeOptions).toEqual([
    { arrange_1p: 'random', arrange_2p: 'mirror' },
  ])
  await db.update(schema.courseScores).set({ arrangeOptions: [] })
  expect(await submitCourseScore(user, validateCourseScoreSubmission(payload))).toMatchObject({
    course_score_id: submitted.course_score_id,
  })
  expect((await db.select().from(schema.courseScores))[0]?.arrangeOptions).toEqual([])
})

test('course CB retains unplayed notes and excludes only recorded empty POORs', () => {
  expect(courseComboBreaks(275, { bad: 0, poor: 13, empty_poor: 2 })).toBe(273)
  expect(courseComboBreaks(2, { empty_poor: 2 })).toBe(0)
  expect(courseComboBreaks(5, {})).toBeNull()
  expect(courseComboBreaks(5, { empty_poor: 6 })).toBeNull()
})

test.each([false, true])(
  'keeps a concurrent score when cleanup commits first=%s',
  async (cleanupFirst) => {
    const deleted = submission()
    deleted.play_options.submission_source = 'local_backfill'
    deleted.play_options.source_kind = 'lr2'
    const backfill = await submitScore(user, validateScoreSubmission(deleted), [], 10)
    const next = submission()
    next.idempotency_key = 'after-backfill'
    next.result.ex_score = 150
    next.result.judges.fast.pgreat = 50
    next.result.judges.fast.great = 50
    const scoreIds = [backfill.score_id!]
    const commit = db.batch.bind(db)
    const batch = spyOn(db, 'batch').mockImplementationOnce(async (statements) => {
      if (cleanupFirst) {
        await deleteLocalBackfillScores(user, scoreIds)
      } else {
        await submitScore(user, validateScoreSubmission(next), [], 10)
      }
      return commit(statements)
    })
    try {
      if (cleanupFirst) {
        await submitScore(user, validateScoreSubmission(next), [], 10)
      } else {
        expect((await deleteLocalBackfillScores(user, scoreIds)).deleted_score_ids).toEqual(
          scoreIds,
        )
      }
    } finally {
      batch.mockRestore()
    }
    const [history] = await db.select().from(schema.scores)
    const best = await db.select().from(schema.bestScores)
    expect(history?.id).not.toBe(backfill.score_id)
    expect(best).toHaveLength(1)
    expect(best[0]).toMatchObject({ exScore: 150, scoreId: history?.id })
  },
)

test('cleanup can commit across an attestation and rebuilds the latest verification', async () => {
  const deleted = submission()
  deleted.play_options.submission_source = 'local_backfill'
  deleted.play_options.source_kind = 'lr2'
  const backfill = await submitScore(user, validateScoreSubmission(deleted), [], 10)
  const kept = submission()
  kept.idempotency_key = 'attested-while-cleaning'
  const keptScore = await submitScore(user, validateScoreSubmission(kept), [], 10)
  const commit = db.batch.bind(db)
  const batch = spyOn(db, 'batch').mockImplementationOnce(async (statements) => {
    await db
      .update(schema.scores)
      .set({ evidence: { attestation: {} }, verification: 'verified_play' })
      .where(eq(schema.scores.id, keptScore.score_id!))
    return commit(statements)
  })
  try {
    expect((await deleteLocalBackfillScores(user, [backfill.score_id!])).deleted_score_ids).toEqual(
      [backfill.score_id],
    )
    expect(batch).toHaveBeenCalledTimes(1)
  } finally {
    batch.mockRestore()
  }
  expect((await db.select().from(schema.bestScores))[0]).toMatchObject({
    scoreId: keptScore.score_id,
    verification: 'verified_play',
  })
})

test('a later course upsert cannot replace a concurrently committed higher EX score', async () => {
  const low = courseSubmission()
  const high = courseSubmission()
  high.idempotency_key = 'course-high'
  high.result.ex_score = 180

  const execute = client.execute.bind(client)
  let startedHigher = false
  const spy = spyOn(client, 'execute').mockImplementation(async (...args) => {
    const statement = args[0]
    if (
      !startedHigher &&
      typeof statement === 'object' &&
      statement.sql.startsWith('insert into "best_course_scores"')
    ) {
      startedHigher = true
      await submitCourseScore(user, validateCourseScoreSubmission(high))
    }
    return execute(...args)
  })
  try {
    const result = await submitCourseScore(user, validateCourseScoreSubmission(low))
    expect(startedHigher).toBe(true)
    expect(result.best_updated).toBe(false)
  } finally {
    spy.mockRestore()
  }
  expect(await db.select().from(schema.courseScores)).toHaveLength(2)
  expect((await db.select().from(schema.bestCourseScores))[0]?.exScore).toBe(180)
})

test('new normalized course FORCE and historical AUTO remain accepted in separate buckets', async () => {
  for (const policy of ['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn']) {
    const payload = courseSubmission()
    payload.rule.ln_policy = policy
    payload.idempotency_key = `uniform-course-${policy}`
    expect((await submitCourseScore(user, validateCourseScoreSubmission(payload))).accepted).toBe(
      true,
    )
  }
  const scores = await db.select().from(schema.courseScores)
  expect(scores.map((row) => row.lnPolicy).sort()).toEqual(
    ['AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn'].sort(),
  )
})

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
  test.each([undefined, 0])(
    'Kaleid version %s upgrades without storing per-play chart state',
    async (version) => {
      const legacy = submission()
      if (version !== undefined) {
        legacy.chart.kaleid = { version, declared: {}, source: {}, effective: {}, diagnostics: [] }
      }
      await submitScore(user, validateScoreSubmission(legacy), [], 10)
      const current = structuredClone(legacy)
      current.idempotency_key += '-known'
      current.chart.kaleid = {
        version: 1,
        declared: { conditional: true },
        source: { conditional: true, hln: true },
        effective: { hln: false },
        diagnostics: ['partial selected branch'],
      }
      current.play_options.kaleid = { provenance: current.chart.kaleid }
      await submitScore(secondUser, validateScoreSubmission(current), [], 10)
      const expected = {
        version: 1,
        declared: current.chart.kaleid.declared,
        source: current.chart.kaleid.source,
      }
      expect((await db.select().from(schema.charts))[0]!.kaleid).toEqual(expected)
      expect(
        (await db.select().from(schema.scores)).find((row) => row.playerId === 'second')!
          .playOptions.kaleid,
      ).toEqual(current.play_options.kaleid)
      legacy.idempotency_key += '-later-legacy'
      await submitScore(user, validateScoreSubmission(legacy), [], 10)
      expect((await db.select().from(schema.charts))[0]!.kaleid).toEqual(expected)
    },
  )

  test('known chart provenance omits and cleans up effective features and diagnostics', async () => {
    const first = submission()
    first.chart.kaleid = {
      version: 1,
      declared: { hln: true },
      source: { hln: true },
      effective: { hln: false },
      diagnostics: ['first play'],
    }
    await submitScore(user, validateScoreSubmission(first), [], 10)
    const expected = { version: 1, declared: { hln: true }, source: { hln: true } }
    expect((await db.select().from(schema.charts))[0]!.kaleid).toEqual(expected)
    // Rows written by the earlier implementation are cleaned when touched again.
    await db.update(schema.charts).set({ kaleid: { ...first.chart.kaleid } })
    const next = structuredClone(first)
    next.idempotency_key += '-next'
    next.chart.kaleid!.effective = { hln: true }
    next.chart.kaleid!.diagnostics = []
    await submitScore(secondUser, validateScoreSubmission(next), [], 10)
    expect((await db.select().from(schema.charts))[0]!.kaleid).toEqual(expected)
  })

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
    const firstPage = await fetchCourseRankingPage(
      [eq(schema.courseScores.courseHash, payload.course.course_hash)],
      null,
      2,
    )
    expect(firstPage.entries.map(({ row }) => [row.player_id, row.ex_score])).toEqual([
      ['player', 200],
      ['second', 100],
    ])
    // Self rank and total include players outside the returned entries.
    const ranked = await fetchCourseRankingPage(
      [eq(schema.courseScores.courseHash, payload.course.course_hash)],
      'second',
      1,
    )
    expect(ranked.entries.map(({ rank, row }) => [rank, row.player_id])).toEqual([[1, 'player']])
    expect(ranked.self && [ranked.self.rank, ranked.self.row.player_id]).toEqual([2, 'second'])
    expect(ranked.total).toBe(2)
  })
  test.each([false, true])(
    'concurrent submission reports only the fields it changes (better clear=%s)',
    async (betterClear) => {
      const high = validateScoreSubmission(submission())
      const lowPayload = submission()
      lowPayload.idempotency_key = 'lower-score'
      lowPayload.result.ex_score = 100
      lowPayload.result.judges.fast.pgreat = 0
      lowPayload.result.judges.fast.great = 100
      if (betterClear) {
        lowPayload.result.clear = 'Hard'
        lowPayload.rule.gauge = 'hard'
      }
      const low = validateScoreSubmission(lowPayload)
      const commit = db.batch.bind(db)
      let firstId: string | undefined
      const batch = spyOn(db, 'batch').mockImplementationOnce(async (statements) => {
        firstId = (await submitScore(user, high, [], 10)).score_id
        return commit(statements)
      })
      try {
        const second = await submitScore(user, low, [], 10)
        expect(second).toMatchObject({
          best_updated: betterClear,
          updated_fields: {
            ex_score: false,
            clear: betterClear,
            max_combo: false,
            min_bp: false,
            min_cb: false,
          },
          previous_best: {
            clear_type: high.result.clear,
            ex_score: 200,
            max_combo: 100,
            min_bp: 0,
            min_cb: 0,
          },
        })
        expect((await db.select().from(schema.bestScores))[0]).toMatchObject({
          scoreId: firstId,
          bestExScoreId: firstId,
          exScore: 200,
          bestClearScoreId: betterClear ? second.score_id : firstId,
          clearType: betterClear ? low.result.clear : high.result.clear,
          gauge: high.rule.gauge,
        })
      } finally {
        batch.mockRestore()
      }
    },
  )
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

describe('BATTLE AS auto scratch scoring', () => {
  function battleAs(key: string, marked: boolean): IrScoreSubmission {
    const payload = submission()
    payload.idempotency_key = key
    payload.play_options.double_option = 'battle_auto_scratch'
    payload.play_options.applied_double_option = 'battle_auto_scratch'
    if (marked) payload.play_options.auto_scratch_scoring = 'unscored'
    return validateScoreSubmission(payload)
  }
  const battleAsRanking = parseRankingQuery({
    scope: 'global',
    limit: 10,
    double_option: 'battle_auto_scratch',
  })

  test('an unmarked BATTLE AS score stays history without a best or a ranking entry', async () => {
    const response = await submitScore(user, battleAs('legacy', false), [], 10)
    const retry = await submitScore(user, battleAs('legacy', false), [], 10)

    expect(response).toMatchObject({ accepted: false, best_updated: false })
    expect(retry).toMatchObject({ accepted: false, score_id: response.score_id })
    expect(await db.select().from(schema.scores)).toMatchObject([
      { accepted: false, rejectionReason: 'battle_auto_scratch_legacy_scoring' },
    ])
    expect(await db.select().from(schema.bestScores)).toEqual([])
    const ranking = await getRanking(user, 'a'.repeat(64), battleAsRanking)
    expect(ranking.ranking.entries).toEqual([])
  })

  test('a marked BATTLE AS score is accepted into its own bucket', async () => {
    const response = await submitScore(user, battleAs('unscored', true), [], 10)

    expect(response).toMatchObject({ accepted: true, best_updated: true })
    expect(await db.select().from(schema.bestScores)).toMatchObject([
      { doubleOption: 'battle_auto_scratch', scoreId: response.score_id },
    ])
    const ranking = await getRanking(user, 'a'.repeat(64), battleAsRanking)
    expect(ranking.ranking.entries).toMatchObject([{ score: { score_id: response.score_id } }])
  })

  test('an imported BATTLE AS score without the BMZ scoring marker is accepted', async () => {
    const payload = battleAs('imported-battle-as', false)
    payload.play_options.source_kind = 'beatoraja'
    const result = await submitScore(user, payload, [], 10)
    expect(result).toMatchObject({ accepted: true, best_updated: true })
    const replay = await submitScore(user, payload, [], 10)
    expect(replay).toMatchObject({ accepted: true, score_id: result.score_id })
  })

  test('migration 0020 removes stored legacy BATTLE AS scores from bests', async () => {
    const legacy = await submitScore(user, battleAs('stored-legacy', true), [], 10)
    const normal = await submitScore(user, validateScoreSubmission(submission()), [], 10)
    // Rows stored before the marker existed have no auto_scratch_scoring.
    await db
      .update(schema.scores)
      .set({ playOptions: { device_type: 'keyboard', double_option: 'battle_auto_scratch' } })
      .where(eq(schema.scores.id, legacy.score_id!))

    const migration = new URL('0020_battle_as_legacy_scoring.sql', migrations)
    await client.executeMultiple(await readFile(migration, 'utf8'))

    const rows = await db.select().from(schema.scores)
    expect(rows.find((row) => row.id === legacy.score_id)).toMatchObject({
      accepted: false,
      rejectionReason: 'battle_auto_scratch_legacy_scoring',
    })
    expect(rows.find((row) => row.id === normal.score_id)).toMatchObject({ accepted: true })
    expect(await db.select().from(schema.bestScores)).toMatchObject([
      { doubleOption: 'off', scoreId: normal.score_id },
    ])
  })

  test('migration 0021 restores 0020 imports and rebuilds independent bests', async () => {
    const legacy = await submitScore(user, battleAs('restore-legacy', true), [], 10)
    const validPayload = battleAs('restore-marked', true)
    Object.assign(validPayload.result, {
      clear: 'Failed',
      ex_score: 196,
      max_combo: 98,
      min_bp: 2,
      min_cb: 2,
    })
    Object.assign(validPayload.result.judges.fast, { pgreat: 98, bad: 2 })
    const valid = await submitScore(user, validateScoreSubmission(validPayload), [], 10)
    const importPayload = battleAs('restore-import', false)
    importPayload.play_options.source_kind = 'beatoraja'
    importPayload.result.ex_score = 100
    Object.assign(importPayload.result.judges.fast, { pgreat: 0, great: 100 })
    const imported = await submitScore(user, validateScoreSubmission(importPayload), [], 10)
    await db
      .update(schema.scores)
      .set({ playOptions: { double_option: 'battle_auto_scratch' } })
      .where(eq(schema.scores.id, legacy.score_id!))

    await client.executeMultiple(
      await readFile(new URL('0020_battle_as_legacy_scoring.sql', migrations), 'utf8'),
    )
    expect(await db.select().from(schema.bestScores)).toEqual([])
    const excluded = await db.select().from(schema.scores)
    expect(excluded.find((row) => row.id === imported.score_id)?.accepted).toBe(false)

    await client.executeMultiple(
      await readFile(new URL('0021_restore_imported_battle_as_bests.sql', migrations), 'utf8'),
    )
    const rows = await db.select().from(schema.scores)
    expect(rows.find((row) => row.id === imported.score_id)).toMatchObject({
      accepted: true,
      rejectionReason: null,
    })
    expect(rows.find((row) => row.id === legacy.score_id)?.accepted).toBe(false)
    expect(rows.find((row) => row.id === valid.score_id)?.accepted).toBe(true)
    const best = await db.select().from(schema.bestScores)
    expect(best).toHaveLength(1)
    expect(best[0]).toMatchObject({
      scoreId: valid.score_id,
      bestExScoreId: valid.score_id,
      bestClearScoreId: imported.score_id,
      bestMaxComboScoreId: imported.score_id,
      bestMinBpScoreId: imported.score_id,
      bestMinCbScoreId: imported.score_id,
    })
    const replay = await submitScore(user, importPayload, [], 10)
    expect(replay).toMatchObject({ accepted: true, score_id: imported.score_id })
  })
})
