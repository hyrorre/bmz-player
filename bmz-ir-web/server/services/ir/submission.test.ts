import { afterAll, beforeEach, describe, expect, mock, spyOn, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { eq } from 'drizzle-orm'
import { readdir, readFile } from 'node:fs/promises'
import * as schema from '../../db/schema'
import type { IrScoreSubmission } from '../../../shared/types/ir'

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

describe('shared submission metadata', () => {
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
