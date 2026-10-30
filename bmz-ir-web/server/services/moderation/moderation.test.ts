import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test'
import { createClient } from '@libsql/client'
import { drizzle } from 'drizzle-orm/libsql'
import { eq } from 'drizzle-orm'
import * as h3 from 'h3'
import type { H3Event } from 'h3'
import { readdir, readFile } from 'node:fs/promises'
import * as schema from '../../db/schema'
import type { IrScoreSubmission } from '../../../shared/types/ir'

// 実際の migration を適用した in-memory SQLite で、認証・送信・公開読み取り・管理操作を通して確認する。
const client = createClient({ url: 'file::memory:' })
const db = drizzle(client, { schema })
mock.module('hub:db', () => ({ db, schema }))

const runtimeConfig = { ir: { adminUserIds: 'admin', providerKey: 'bmz-test' } }
Object.assign(globalThis, {
  defineEventHandler: (handler: unknown) => handler,
  getRouterParam: h3.getRouterParam,
  getRequestURL: h3.getRequestURL,
  getHeader: h3.getHeader,
  readBody: h3.readBody,
  createError: h3.createError,
  useRuntimeConfig: () => runtimeConfig,
  getUserSession: async () => ({}),
  setUserSession: async () => undefined,
  clearUserSession: async () => undefined,
  verifyPassword: async (hash: string, password: string) => hash === `hash:${password}`,
})

const migrations = new URL('../../../../server/db/migrations/sqlite/', import.meta.url)
for (const name of (await readdir(migrations)).filter((name) => name.endsWith('.sql')).sort()) {
  await client.executeMultiple(await readFile(new URL(name, migrations), 'utf8'))
}

const { submitScore } = await import('../ir/submission')
const { getRanking, parseRankingQuery } = await import('../ir')
const { submitCourseScore, computeCourseHash } = await import('../course_ir')
const { loadDailyReport } = await import('../daily_report')
const { loadRivalComparison, loadRivalLists, rivalTargetExists } = await import('../rivals')
const { lookupChartSha256ByMd5 } = await import('../charts')
const {
  createAuthTokens,
  findUserByAccessToken,
  findUserByWebSession,
  revokeToken,
  revokeUserSessions,
  rotateRefreshToken,
} = await import('../../utils/auth_tokens')
const { resolveIrUser, requireIrUser } = await import('../../utils/auth')
const { requireIrAdmin } = await import('../../utils/admin')
const { IrModerationError, assertCanSubmit } = await import('./enforcement')
const { changeChartModeration, changeUserModeration, listAuditLogs, searchCharts, searchUsers } =
  await import('./admin')
const loginHandler = (await import('../../api/v1/auth/login.post')).default as Handler
const refreshHandler = (await import('../../api/v1/auth/refresh.post')).default as Handler
const scoresPostHandler = (await import('../../api/v1/scores.post')).default as Handler
const chartsIndexHandler = (await import('../../api/v1/charts/index.get')).default as Handler
const chartDetailHandler = (await import('../../api/v1/charts/[sha256]/index.get'))
  .default as Handler
const playersIndexHandler = (await import('../../api/v1/players/index.get')).default as Handler
const playerDetailHandler = (await import('../../api/v1/players/[id]/index.get')).default as Handler
const scoreDetailHandler = (await import('../../api/v1/scores/[id]/index.get')).default as Handler
const replayHandler = (await import('../../api/v1/scores/[id]/replay/index.get')).default as Handler
const courseRankingHandler = (await import('../../api/v1/courses/[hash]/ranking.get'))
  .default as Handler
const coursesIndexHandler = (await import('../../api/v1/courses/index.get')).default as Handler
const userStatusHandler = (await import('../../api/v1/admin/moderation/users/[id]/status.post'))
  .default as Handler
const chartStatusHandler = (await import('../../api/v1/admin/moderation/charts/status.post'))
  .default as Handler
const auditLogsHandler = (await import('../../api/v1/admin/moderation/audit-logs.get'))
  .default as Handler
const meHandler = (await import('../../api/v1/me.get')).default as Handler

// 応答の形はテストごとに検証するため、ハンドラーの戻り値は緩く扱う。
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Handler = (event: H3Event) => Promise<any>

const CHART_A = 'a'.repeat(64)
const CHART_B = 'b'.repeat(64)
const MD5_B = 'b'.repeat(32)
const ADMIN_IDS = new Set(['admin'])

afterAll(() => client.close())

beforeEach(async () => {
  // 監査ログは追記専用 (DELETE トリガーで拒否) なので、テスト間の初期化は他のテーブルだけ行う。
  await db.delete(schema.replayObjects)
  await db.delete(schema.rivalRelationships)
  await db.delete(schema.bestCourseScores)
  await db.delete(schema.courseScores)
  await db.delete(schema.irCourses)
  await db.delete(schema.bestScores)
  await db.delete(schema.scores)
  await db.delete(schema.charts)
  await db.delete(schema.userModeration)
  await db.delete(schema.chartModeration)
  await db.delete(schema.authRateLimits)
  await db.delete(schema.sessions)
  await db.delete(schema.profiles)
  await db.delete(schema.users)
  for (const id of ['admin', 'alice', 'bob', 'carol']) {
    await db
      .insert(schema.users)
      .values({ id, email: `${id}@example.invalid`, passwordHash: `hash:${id}-password` })
    await db.insert(schema.profiles).values({ id, displayName: id.toUpperCase() })
  }
})

function event(options: {
  method?: string
  path?: string
  params?: Record<string, string>
  body?: Record<string, unknown>
  token?: string
}): H3Event {
  const headers: Record<string, string> = { host: 'ir.example.invalid' }
  if (options.token) headers.authorization = `Bearer ${options.token}`
  if (options.body) headers['content-type'] = 'application/json'
  return {
    method: options.method ?? 'GET',
    path: options.path ?? '/',
    context: { params: options.params ?? {} },
    node: {
      req: {
        method: options.method ?? 'GET',
        url: options.path ?? '/',
        headers,
        body: options.body,
        socket: { remoteAddress: '127.0.0.1' },
      },
      res: { setHeader() {}, getHeader() {} },
    },
  } as unknown as H3Event
}

async function expectHttpError(promise: Promise<unknown>, statusCode: number, code?: string) {
  try {
    await promise
  } catch (error) {
    const httpError = error as { statusCode?: number; data?: { code?: string } }
    expect(httpError.statusCode).toBe(statusCode)
    if (code) expect(httpError.data?.code).toBe(code)
    return
  }
  throw new Error(`expected HTTP ${statusCode}`)
}

function submission(
  overrides: { sha256?: string; md5?: string; exScore?: number; key?: string; clear?: string } = {},
): IrScoreSubmission {
  const counts = { pgreat: 0, great: 0, good: 0, bad: 0, poor: 0, empty_poor: 0 }
  const exScore = overrides.exScore ?? 200
  return {
    client: { name: 'bmz', version: '0.3.0', platform: 'windows' },
    chart: {
      sha256: overrides.sha256 ?? CHART_A,
      md5: overrides.md5,
      title: overrides.sha256 === CHART_B ? 'Blocked Song' : 'Normal Song',
    },
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
      clear: overrides.clear ?? 'clear',
      played_at: Math.floor(Date.now() / 1000),
      judges: { fast: { ...counts, pgreat: Math.floor(exScore / 2) }, slow: counts },
      ex_score: exScore,
      max_combo: 100,
      notes: 100,
      min_bp: 0,
      min_cb: 0,
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: overrides.key ?? `key-${Math.random()}`,
  }
}

function rankingQuery() {
  return parseRankingQuery({ ln_policy: 'ForceLn' })
}

async function setUserStatus(userId: string, status: 'active' | 'submit_suspended' | 'banned') {
  const current = await db.query.userModeration.findFirst({
    where: eq(schema.userModeration.userId, userId),
  })
  const result = await changeUserModeration({
    actorId: 'admin',
    adminUserIds: ADMIN_IDS,
    userId,
    status,
    reason: 'test reason',
    expectedRevision: current?.revision ?? 0,
  })
  expect(result.result).toBe('changed')
  return result
}

async function setChartStatus(
  sha256: string,
  status: 'active' | 'under_review' | 'hidden' | 'blocked',
) {
  const current = await db.query.chartModeration.findFirst({
    where: eq(schema.chartModeration.chartSha256, sha256),
  })
  const [result] = await changeChartModeration({
    actorId: 'admin',
    items: [{ sha256, expectedRevision: current?.revision ?? 0 }],
    status,
    reason: 'copyright report',
  })
  expect(result!.result).toBe('changed')
}

async function expectModerationError(promise: Promise<unknown>, code: string) {
  try {
    await promise
  } catch (error) {
    expect(error).toBeInstanceOf(IrModerationError)
    expect((error as InstanceType<typeof IrModerationError>).code).toBe(code)
    return
  }
  throw new Error(`expected ${code}`)
}

describe('account moderation and authentication', () => {
  test('active users authenticate, submit, and see their account status', async () => {
    const tokens = await createAuthTokens('alice')
    expect(await requireIrUser(event({ token: tokens.accessToken }))).toMatchObject({ id: 'alice' })
    const response = await submitScore({ id: 'alice' }, submission(), [], 10)
    expect(response.accepted).toBeTrue()
    const me = await meHandler(event({ token: tokens.accessToken }))
    expect(me.player).toMatchObject({ account_status: 'active', is_admin: false })
  })

  test('submit_suspended users can log in and read but cannot submit or resend', async () => {
    const prior = submission({ key: 'before-suspension' })
    await submitScore({ id: 'alice' }, prior, [], 10)
    await setUserStatus('alice', 'submit_suspended')

    const login = await loginHandler(
      event({
        method: 'POST',
        body: { email: 'alice@example.invalid', password: 'alice-password' },
      }),
    )
    expect(login.access_token).toBeString()
    const me = await meHandler(event({ token: login.access_token }))
    expect(me.player.account_status).toBe('submit_suspended')

    await expectModerationError(
      submitScore({ id: 'alice' }, submission(), [], 10),
      'ACCOUNT_SUSPENDED',
    )
    // idempotency key による既存スコアの成功応答でも回避できない。
    await expectModerationError(submitScore({ id: 'alice' }, prior, [], 10), 'ACCOUNT_SUSPENDED')
    // local backfill (一括送信) も同じ経路で拒否される。
    const backfill = submission()
    backfill.play_options = { device_type: 'keyboard', submission_source: 'local_backfill' }
    await expectHttpError(
      scoresPostHandler(
        event({ method: 'POST', body: { ...backfill }, token: login.access_token }),
      ),
      403,
      'ACCOUNT_SUSPENDED',
    )
    // 既存の公開スコアは維持する。
    const ranking = await getRanking(null, CHART_A, rankingQuery())
    expect(ranking.ranking.entries.map((entry) => entry.player.id)).toEqual(['alice'])
  })

  test('banned users lose access, refresh, cookie sessions, and login until restored', async () => {
    const desktop = await createAuthTokens('alice', { clientType: 'desktop' })
    const web = await createAuthTokens('alice')
    await setUserStatus('alice', 'banned')

    // access token は失効し、public API では未ログイン扱いになる。
    expect(await resolveIrUser(event({ token: desktop.accessToken }))).toBeNull()
    await expectHttpError(requireIrUser(event({ token: desktop.accessToken })), 401)
    // refresh token は識別可能なエラーで拒否され、新しい token は発行されない。
    await expectHttpError(
      refreshHandler(event({ method: 'POST', body: { refresh_token: desktop.refreshToken } })),
      403,
      'ACCOUNT_BANNED',
    )
    expect(await findUserByWebSession('alice', web.sessionGroupId)).toBeNull()
    await expectHttpError(
      loginHandler(
        event({
          method: 'POST',
          body: { email: 'alice@example.invalid', password: 'alice-password' },
        }),
      ),
      403,
      'ACCOUNT_BANNED',
    )
    // 誤ったパスワードでは BAN の有無を返さない。
    await expectHttpError(
      loginHandler(
        event({ method: 'POST', body: { email: 'alice@example.invalid', password: 'wrong' } }),
      ),
      401,
    )

    // BAN と同時に発行された token が残る競合でも、現在の状態で拒否する。
    const raced = await createAuthTokens('alice')
    await expectHttpError(requireIrUser(event({ token: raced.accessToken })), 403, 'ACCOUNT_BANNED')
    expect(await rotateRefreshToken(raced.refreshToken)).toEqual({ banned: true })
    expect(await findUserByWebSession('alice', raced.sessionGroupId)).toBeNull()

    await setUserStatus('alice', 'active')
    const login = await loginHandler(
      event({
        method: 'POST',
        body: { email: 'alice@example.invalid', password: 'alice-password' },
      }),
    )
    expect(await requireIrUser(event({ token: login.access_token }))).toMatchObject({ id: 'alice' })
    // 失効済みのセッションは復活させない。
    expect(await findUserByWebSession('alice', web.sessionGroupId)).toBeNull()
  })

  test('a refresh token revoked by a ban never revokes sessions created after the ban is lifted', async () => {
    // BAN前に別端末へ残っていた refresh token。
    const oldDevice = await createAuthTokens('alice', { clientType: 'desktop' })
    await setUserStatus('alice', 'banned')
    // BAN中は refresh も login も拒否する。
    await expectHttpError(
      refreshHandler(event({ method: 'POST', body: { refresh_token: oldDevice.refreshToken } })),
      403,
      'ACCOUNT_BANNED',
    )
    await setUserStatus('alice', 'active')
    const login = await loginHandler(
      event({
        method: 'POST',
        body: { email: 'alice@example.invalid', password: 'alice-password' },
      }),
    )

    // 古い token は解除後も復活しないが、再利用検知として新しいセッションを巻き込まない。
    expect(await rotateRefreshToken(oldDevice.refreshToken)).toBeNull()
    await expectHttpError(
      refreshHandler(event({ method: 'POST', body: { refresh_token: oldDevice.refreshToken } })),
      401,
    )
    expect(await findUserByAccessToken(oldDevice.accessToken)).toBeNull()
    expect(await requireIrUser(event({ token: login.access_token }))).toMatchObject({ id: 'alice' })
    const rotated = await rotateRefreshToken(login.refresh_token)
    expect(rotated && 'tokens' in rotated).toBe(true)
  })

  test('only rotated-token reuse revokes every session; logout and password changes just reject', async () => {
    const now = Date.now()
    const current = await createAuthTokens('alice', { clientType: 'desktop', now })

    const loggedOut = await createAuthTokens('alice', { clientType: 'desktop', now })
    await revokeToken(loggedOut.refreshToken, 'refresh', 'logout', now)
    expect(await rotateRefreshToken(loggedOut.refreshToken, now + 1000)).toBeNull()

    const passwordChanged = await createAuthTokens('alice', { clientType: 'desktop', now })
    await revokeToken(passwordChanged.refreshToken, 'refresh', 'password_changed', now)
    expect(await rotateRefreshToken(passwordChanged.refreshToken, now + 1000)).toBeNull()
    expect(await findUserByAccessToken(current.accessToken, now + 1000)).toMatchObject({
      id: 'alice',
    })

    // rotation 直後の並行リトライは、拒否するだけで他のセッションを残す。
    const rotating = await createAuthTokens('alice', { clientType: 'desktop', now })
    const next = await rotateRefreshToken(rotating.refreshToken, now)
    expect(next && 'tokens' in next).toBe(true)
    expect(await rotateRefreshToken(rotating.refreshToken, now + 60_000)).toBeNull()
    expect(await findUserByAccessToken(current.accessToken, now + 60_000)).toMatchObject({
      id: 'alice',
    })

    // 猶予を過ぎた rotation 済み token の再利用は盗用として全セッションを失効させる。
    expect(await rotateRefreshToken(rotating.refreshToken, now + 10 * 60_000)).toEqual({
      reuseDetected: true,
    })
    expect(await findUserByAccessToken(current.accessToken, now + 10 * 60_000)).toBeNull()
    const nextTokens = next && 'tokens' in next ? next.tokens : null
    expect(await rotateRefreshToken(nextTokens!.refreshToken, now + 10 * 60_000)).toEqual({
      reuseDetected: true,
    })

    // 管理者が失効させたセッションも、新しいログインを巻き込まない。
    const fresh = await createAuthTokens('alice', { clientType: 'desktop', now })
    const revokedByAdmin = await createAuthTokens('bob', { clientType: 'desktop', now })
    await revokeUserSessions('bob', 'admin', now)
    const bob = await createAuthTokens('bob', { clientType: 'desktop', now })
    expect(await rotateRefreshToken(revokedByAdmin.refreshToken, now + 1000)).toBeNull()
    expect(await findUserByAccessToken(bob.accessToken, now + 1000)).toMatchObject({ id: 'bob' })
    expect(await findUserByAccessToken(fresh.accessToken, now + 1000)).toMatchObject({
      id: 'alice',
    })
  })

  test('administrator APIs reject anonymous, regular, and unconfigured access', async () => {
    const regular = await createAuthTokens('alice')
    const admin = await createAuthTokens('admin')
    await expectHttpError(requireIrAdmin(event({})), 401)
    await expectHttpError(requireIrAdmin(event({ token: regular.accessToken })), 403)
    expect(await requireIrAdmin(event({ token: admin.accessToken }))).toMatchObject({ id: 'admin' })
    await expectHttpError(
      userStatusHandler(
        event({
          method: 'POST',
          params: { id: 'bob' },
          body: { status: 'banned', reason: 'x', expected_revision: 0, actor_id: 'admin' },
          token: regular.accessToken,
        }),
      ),
      403,
    )
    expect(await db.select().from(schema.userModeration)).toHaveLength(0)
    await expectHttpError(auditLogsHandler(event({ token: regular.accessToken })), 403)

    const previous = runtimeConfig.ir.adminUserIds
    runtimeConfig.ir.adminUserIds = ''
    try {
      await expectHttpError(requireIrAdmin(event({ token: admin.accessToken })), 403)
    } finally {
      runtimeConfig.ir.adminUserIds = previous
    }
  })

  test('administrators cannot restrict themselves or other administrators', async () => {
    const admin = await createAuthTokens('admin')
    await expectHttpError(
      userStatusHandler(
        event({
          method: 'POST',
          params: { id: 'admin' },
          body: { status: 'banned', reason: 'oops', expected_revision: 0 },
          token: admin.accessToken,
        }),
      ),
      403,
    )
    await expect(
      changeUserModeration({
        actorId: 'alice',
        adminUserIds: ADMIN_IDS,
        userId: 'admin',
        status: 'submit_suspended',
        reason: 'x',
        expectedRevision: 0,
      }),
    ).rejects.toMatchObject({ statusCode: 403 })
    expect(await db.select().from(schema.userModeration)).toHaveLength(0)
  })
})

describe('chart moderation', () => {
  test('active and under_review charts accept scores and remain public', async () => {
    await submitScore({ id: 'alice' }, submission(), [], 10)
    await setChartStatus(CHART_A, 'under_review')
    await submitScore({ id: 'bob' }, submission({ exScore: 150 }), [], 10)
    const ranking = await getRanking(null, CHART_A, rankingQuery())
    expect(ranking.ranking.entries.map((entry) => entry.player.id)).toEqual(['alice', 'bob'])
  })

  test('blocked charts reject new scores, idempotent resends, backfills, and courses', async () => {
    const prior = submission({ sha256: CHART_B, key: 'before-block' })
    await submitScore({ id: 'alice' }, prior, [], 10)
    await setChartStatus(CHART_B, 'blocked')
    const before = await db.select().from(schema.scores)

    await expectModerationError(
      submitScore({ id: 'bob' }, submission({ sha256: CHART_B }), [], 10),
      'CHART_BLOCKED',
    )
    await expectModerationError(
      submitScore({ id: 'alice' }, prior, ['global'], 10),
      'CHART_BLOCKED',
    )
    const backfill = submission({ sha256: CHART_B })
    backfill.play_options = { device_type: 'keyboard', submission_source: 'local_backfill' }
    const token = await createAuthTokens('bob')
    await expectHttpError(
      scoresPostHandler(event({ method: 'POST', body: { ...backfill }, token: token.accessToken })),
      403,
      'CHART_BLOCKED',
    )
    // 未登録の譜面を事前ブロックした場合も charts / scores / best_scores を作らない。
    const unregistered = 'c'.repeat(64)
    await setChartStatus(unregistered, 'blocked')
    await expectModerationError(
      submitScore({ id: 'bob' }, submission({ sha256: unregistered }), [], 10),
      'CHART_BLOCKED',
    )
    expect(await db.select().from(schema.scores)).toEqual(before)
    expect(
      await db.select().from(schema.charts).where(eq(schema.charts.sha256, unregistered)),
    ).toHaveLength(0)

    const charts = [CHART_A, CHART_B]
    await expectModerationError(
      submitCourseScore({ id: 'bob' }, coursePayload(charts, 'course-blocked')),
      'CHART_BLOCKED',
    )
    expect(await db.select().from(schema.courseScores)).toHaveLength(0)
  })

  test('hidden charts reject submissions with a distinct code', async () => {
    await setChartStatus(CHART_A, 'hidden')
    await expectModerationError(submitScore({ id: 'alice' }, submission(), [], 10), 'CHART_HIDDEN')
    await expect(assertCanSubmit('alice', [CHART_B])).resolves.toBeUndefined()
  })

  test('hidden and blocked charts disappear from public reads and return when restored', async () => {
    const aliceA = await submitScore({ id: 'alice' }, submission(), [], 10)
    const aliceB = await submitScore(
      { id: 'alice' },
      submission({ sha256: CHART_B, md5: MD5_B }),
      [],
      10,
    )
    await submitScore(
      { id: 'bob' },
      submission({ sha256: CHART_B, md5: MD5_B, exScore: 100 }),
      [],
      10,
    )
    expect(await lookupChartSha256ByMd5(MD5_B)).toBe(CHART_B)
    await db
      .insert(schema.rivalRelationships)
      .values({ ownerPlayerId: 'bob', targetPlayerId: 'alice' })
    await db.insert(schema.replayObjects).values({
      id: 'replay-b',
      scoreId: aliceB.score_id,
      playerId: 'alice',
      objectPath: 'alice/replay-b',
      hash: 'h',
      format: 'bmz-replay-v1',
      status: 'verified',
    })
    const today = new Date(Date.now() + 9 * 3600_000).toISOString().slice(0, 10)
    const dailyBefore = await loadDailyReport({ playerId: 'alice', date: today, mode: 'all' })
    expect(dailyBefore!.charts).toHaveLength(2)

    for (const status of ['hidden', 'blocked'] as const) {
      await setChartStatus(CHART_B, status)
      const chartList = await chartsIndexHandler(event({ path: '/api/v1/charts' }))
      expect(chartList.charts.map((chart: { sha256: string }) => chart.sha256)).toEqual([CHART_A])
      expect(chartList.pagination.total).toBe(1)
      await expectHttpError(chartDetailHandler(event({ params: { sha256: CHART_B } })), 404)
      expect(await lookupChartSha256ByMd5(MD5_B)).toBeNull()
      expect((await getRanking(null, CHART_B, rankingQuery())).ranking.entries).toHaveLength(0)
      const profile = await playerDetailHandler(event({ params: { id: 'alice' } }))
      expect(profile.best_scores.map((row: { chart_sha256: string }) => row.chart_sha256)).toEqual([
        CHART_A,
      ])
      const players = await playersIndexHandler(event({ path: '/api/v1/players' }))
      const alice = players.players.find((player: { id: string }) => player.id === 'alice')
      expect(alice.best_score_count).toBe(1)
      const daily = await loadDailyReport({ playerId: 'alice', date: today, mode: 'all' })
      expect(daily!.charts.map((chart) => chart.chart.sha256)).toEqual([CHART_A])
      const comparison = await loadRivalComparison('bob', 'alice', 50, 0)
      expect(comparison!.pagination.total).toBe(0)
      await expectHttpError(scoreDetailHandler(event({ params: { id: aliceB.score_id } })), 404)
      await expectHttpError(
        replayHandler(
          event({
            path: `/api/v1/scores/${aliceB.score_id}/replay`,
            params: { id: aliceB.score_id },
          }),
        ),
        404,
      )
      // 本人は自分のスコア詳細を確認できる。
      const own = await createAuthTokens('alice')
      expect(
        (
          await scoreDetailHandler(
            event({ params: { id: aliceB.score_id }, token: own.accessToken }),
          )
        ).score.id,
      ).toBe(aliceB.score_id)
    }

    // 既存スコアは削除しない。
    expect(await db.select().from(schema.scores)).toHaveLength(3)
    expect(await db.select().from(schema.bestScores)).toHaveLength(3)

    await setChartStatus(CHART_B, 'active')
    expect((await getRanking(null, CHART_B, rankingQuery())).ranking.entries).toHaveLength(2)
    expect(await lookupChartSha256ByMd5(MD5_B)).toBe(CHART_B)
    const profile = await playerDetailHandler(event({ params: { id: 'alice' } }))
    expect(profile.best_scores).toHaveLength(2)
    expect(
      (
        await replayHandler(
          event({
            path: `/api/v1/scores/${aliceB.score_id}/replay`,
            params: { id: aliceB.score_id },
          }),
        )
      ).status,
    ).toBe('verified')
    expect((await scoreDetailHandler(event({ params: { id: aliceA.score_id } }))).score.id).toBe(
      aliceA.score_id,
    )
    const daily = await loadDailyReport({ playerId: 'alice', date: today, mode: 'all' })
    expect(daily!.charts).toHaveLength(2)
  })
})

function coursePayload(charts: string[], key: string, exScore = 100) {
  return {
    client: { name: 'BMZ', version: 'test', platform: 'windows' },
    course: { course_hash: computeCourseHash(charts, {}), charts, title: 'Course' },
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
      played_entries: charts.length,
      ex_score: exScore,
      max_ex_score: 400,
      max_combo: 50,
      bp: 5,
      judges: {},
      gauge_value: 80,
      entries: charts.map(() => ({})),
      played_at: 1234567890,
    },
    play_options: { device_type: 'keyboard' },
    idempotency_key: key,
  }
}

describe('rankings, courses, and aggregates', () => {
  test('banned players are excluded before ranking, paging, totals, and clear rate', async () => {
    await submitScore({ id: 'alice' }, submission({ exScore: 300 }), [], 10)
    await submitScore({ id: 'bob' }, submission({ exScore: 200, clear: 'failed' }), [], 10)
    await submitScore({ id: 'carol' }, submission({ exScore: 100 }), [], 10)
    await setUserStatus('alice', 'banned')

    const page = await getRanking(null, CHART_A, { ...rankingQuery(), limit: 1, offset: 0 })
    expect(page.ranking.entries.map((entry) => [entry.player.id, entry.rank])).toEqual([['bob', 1]])
    expect(page.ranking.pagination).toMatchObject({ total: 2, has_more: true })
    expect(page.ranking.clear_rate).toBe(50)
    const second = await getRanking(null, CHART_A, { ...rankingQuery(), limit: 1, offset: 1 })
    expect(second.ranking.entries.map((entry) => [entry.player.id, entry.rank])).toEqual([
      ['carol', 2],
    ])

    const chart = await chartDetailHandler(event({ params: { sha256: CHART_A } }))
    expect(chart.stats.global).toEqual({ play_count: 2, clear_count: 1 })

    const players = await playersIndexHandler(event({ path: '/api/v1/players?limit=2' }))
    expect(players.players.map((player: { id: string }) => player.id)).not.toContain('alice')
    expect(players.pagination.total).toBe(3)
    expect(players.players).toHaveLength(2)
    await expectHttpError(playerDetailHandler(event({ params: { id: 'alice' } })), 404)
    expect(await loadDailyReport({ playerId: 'alice', date: '2026-01-01', mode: 'all' })).toBeNull()
    expect(await rivalTargetExists('alice')).toBeFalse()

    await db.insert(schema.rivalRelationships).values([
      { ownerPlayerId: 'bob', targetPlayerId: 'alice' },
      { ownerPlayerId: 'alice', targetPlayerId: 'bob' },
    ])
    const lists = await loadRivalLists('bob')
    expect(lists.rivals).toHaveLength(0)
    expect(lists.reverse_rivals).toHaveLength(0)
    expect(await loadRivalComparison('bob', 'alice', 50, 0)).toBeNull()

    // 解除するとランキングに戻る (best_scores の再計算は不要)。
    await setUserStatus('alice', 'active')
    const restored = await getRanking(null, CHART_A, rankingQuery())
    expect(restored.ranking.entries.map((entry) => entry.player.id)).toEqual([
      'alice',
      'bob',
      'carol',
    ])
  })

  test('rejected scores stay hidden after an account is restored', async () => {
    await submitScore({ id: 'alice' }, submission({ exScore: 300 }), [], 10)
    await db
      .update(schema.scores)
      .set({ accepted: false })
      .where(eq(schema.scores.playerId, 'alice'))
    await db.delete(schema.bestScores)
    await setUserStatus('alice', 'banned')
    await setUserStatus('alice', 'active')
    const ranking = await getRanking(null, CHART_A, rankingQuery())
    expect(ranking.ranking.entries).toHaveLength(0)
  })

  test('courses with a hidden chart are excluded and restored; banned players leave course rankings', async () => {
    const charts = [CHART_A, CHART_B]
    const hash = computeCourseHash(charts, {})
    await submitCourseScore({ id: 'alice' }, coursePayload(charts, 'course-a', 300))
    await submitCourseScore({ id: 'bob' }, coursePayload(charts, 'course-b', 200))
    await submitCourseScore({ id: 'carol' }, coursePayload(charts, 'course-c', 100))

    await setUserStatus('alice', 'banned')
    const ranking = await courseRankingHandler(
      event({ path: `/api/v1/courses/${hash}/ranking?limit=1`, params: { hash } }),
    )
    expect(ranking.ranking.entries.map((entry) => [entry.player.id, entry.rank])).toEqual([
      ['bob', 1],
    ])
    expect(ranking.ranking.pagination.total).toBe(2)
    const players = await playersIndexHandler(event({ path: '/api/v1/players' }))
    expect(
      players.players.find((p: { id: string }) => p.id === 'bob').best_course_score_count,
    ).toBe(1)

    await setChartStatus(CHART_B, 'hidden')
    await expectHttpError(
      courseRankingHandler(event({ path: `/api/v1/courses/${hash}/ranking`, params: { hash } })),
      404,
    )
    expect((await coursesIndexHandler(event({ path: '/api/v1/courses' }))).courses).toHaveLength(0)
    const hiddenPlayers = await playersIndexHandler(event({ path: '/api/v1/players' }))
    expect(
      hiddenPlayers.players.find((p: { id: string }) => p.id === 'bob').best_course_score_count,
    ).toBe(0)
    await expectModerationError(
      submitCourseScore({ id: 'carol' }, coursePayload(charts, 'course-c')),
      'CHART_HIDDEN',
    )
    expect(await db.select().from(schema.courseScores)).toHaveLength(3)

    await setChartStatus(CHART_B, 'active')
    await setUserStatus('alice', 'active')
    const restored = await courseRankingHandler(
      event({ path: `/api/v1/courses/${hash}/ranking`, params: { hash } }),
    )
    expect(restored.ranking.entries.map((entry) => entry.player.id)).toEqual([
      'alice',
      'bob',
      'carol',
    ])
  })
})

describe('administrator operations and audit logs', () => {
  test('ban, restore, chart block, and bulk changes are audited atomically', async () => {
    const admin = await createAuthTokens('admin')
    const bobSession = await createAuthTokens('bob')
    const banned = await userStatusHandler(
      event({
        method: 'POST',
        params: { id: 'bob' },
        body: { status: 'banned', reason: 'cheating', expected_revision: 0 },
        token: admin.accessToken,
      }),
    )
    expect(banned).toMatchObject({ result: 'changed', status: 'banned', revision: 1 })
    expect(
      (await db.select().from(schema.sessions).where(eq(schema.sessions.userId, 'bob'))).every(
        (session) => session.revokedReason === 'admin',
      ),
    ).toBeTrue()
    expect(
      await requireIrUser(event({ token: bobSession.accessToken })).catch((e) => e.statusCode),
    ).toBe(401)

    // 同じ状態への再変更は unchanged でログを増やさない。
    const repeated = await changeUserModeration({
      actorId: 'admin',
      adminUserIds: ADMIN_IDS,
      userId: 'bob',
      status: 'banned',
      reason: 'again',
      expectedRevision: 1,
    })
    expect(repeated.result).toBe('unchanged')

    const restored = await userStatusHandler(
      event({
        method: 'POST',
        params: { id: 'bob' },
        body: { status: 'active', reason: 'appeal accepted', expected_revision: 1 },
        token: admin.accessToken,
      }),
    )
    expect(restored).toMatchObject({ result: 'changed', status: 'active', revision: 2 })

    await submitScore({ id: 'alice' }, submission(), [], 10)
    await submitScore({ id: 'alice' }, submission({ sha256: CHART_B }), [], 10)
    const bulk = await chartStatusHandler(
      event({
        method: 'POST',
        body: {
          items: [
            { sha256: CHART_A, expected_revision: 0 },
            { sha256: CHART_B, expected_revision: 0 },
          ],
          status: 'blocked',
          reason: 'confirmed infringement',
          note: 'ticket #1',
        },
        token: admin.accessToken,
      }),
    )
    expect(bulk.summary).toEqual({ changed: 2, unchanged: 0, conflict: 0 })
    // 再試行は安全 (unchanged)。古い revision の譜面は conflict として個別に返す。
    const retry = await changeChartModeration({
      actorId: 'admin',
      items: [
        { sha256: CHART_A, expectedRevision: 1 },
        { sha256: CHART_B, expectedRevision: 0 },
      ],
      status: 'blocked',
      reason: 'retry',
    })
    expect(retry.map((item) => item.result)).toEqual(['unchanged', 'conflict'])

    const logs = await auditLogsHandler(
      event({ path: '/api/v1/admin/moderation/audit-logs', token: admin.accessToken }),
    )
    const relevant = logs.logs.filter((log) => ['bob', CHART_A, CHART_B].includes(log.target_id))
    expect(
      relevant.slice(0, 4).map((log) => [log.action, log.previous_status, log.new_status]),
    ).toEqual(
      expect.arrayContaining([
        ['chart_block', 'active', 'blocked'],
        ['chart_block', 'active', 'blocked'],
        ['user_restore', 'banned', 'active'],
        ['user_ban', 'active', 'banned'],
      ]),
    )
    const chartLog = relevant.find((log) => log.target_id === CHART_A)
    expect(chartLog.actor).toEqual({ id: 'admin', display_name: 'ADMIN' })
    expect(chartLog.metadata).toMatchObject({ note: 'ticket #1', batch_size: 2 })
    expect(JSON.stringify(logs)).not.toContain('@example.invalid')

    const filtered = await listAuditLogs({
      targetType: 'user',
      targetId: 'bob',
      limit: 10,
      offset: 0,
    })
    expect(filtered.logs.map((log) => log.action)).toEqual(['user_restore', 'user_ban'])
    const byActor = await listAuditLogs({ actorId: 'nobody', limit: 10, offset: 0 })
    expect(byActor.logs).toHaveLength(0)
  })

  test('invalid input is rejected without changes', async () => {
    const admin = await createAuthTokens('admin')
    const invalidBodies = [
      { status: 'deleted', reason: 'x', expected_revision: 0 },
      { status: 'banned', reason: '   ', expected_revision: 0 },
      { status: 'banned', reason: 'x'.repeat(1001), expected_revision: 0 },
      { status: 'banned', reason: 'x', expected_revision: -1 },
      { status: 'banned', reason: 'x' },
    ]
    for (const body of invalidBodies) {
      await expectHttpError(
        userStatusHandler(
          event({ method: 'POST', params: { id: 'bob' }, body, token: admin.accessToken }),
        ),
        400,
      )
    }
    await expectHttpError(
      userStatusHandler(
        event({
          method: 'POST',
          params: { id: 'missing' },
          body: { status: 'banned', reason: 'x', expected_revision: 0 },
          token: admin.accessToken,
        }),
      ),
      404,
    )
    const invalidCharts = [
      { items: [], status: 'blocked', reason: 'x' },
      { items: [{ sha256: 'xyz', expected_revision: 0 }], status: 'blocked', reason: 'x' },
      {
        items: [
          { sha256: CHART_A, expected_revision: 0 },
          { sha256: CHART_A, expected_revision: 0 },
        ],
        status: 'blocked',
        reason: 'x',
      },
      {
        items: Array.from({ length: 51 }, (_, i) => ({
          sha256: i.toString(16).padStart(64, '0'),
          expected_revision: 0,
        })),
        status: 'blocked',
        reason: 'x',
      },
      { items: [{ sha256: CHART_A, expected_revision: 0 }], status: 'removed', reason: 'x' },
    ]
    for (const body of invalidCharts) {
      await expectHttpError(
        chartStatusHandler(event({ method: 'POST', body, token: admin.accessToken })),
        400,
      )
    }
    expect(await db.select().from(schema.userModeration)).toHaveLength(0)
    expect(await db.select().from(schema.chartModeration)).toHaveLength(0)
  })

  test('stale revisions conflict instead of overwriting newer decisions', async () => {
    const admin = await createAuthTokens('admin')
    await setUserStatus('bob', 'submit_suspended')
    await expectHttpError(
      userStatusHandler(
        event({
          method: 'POST',
          params: { id: 'bob' },
          body: { status: 'active', reason: 'stale screen', expected_revision: 0 },
          token: admin.accessToken,
        }),
      ),
      409,
    )
    // 同時に送られた2つの変更のうち、1つだけが適用されログも1件だけ残る。
    const before = (await listAuditLogs({ targetId: 'carol', limit: 100, offset: 0 })).pagination
      .total
    const results = await Promise.all(
      (['banned', 'submit_suspended'] as const).map((status) =>
        changeUserModeration({
          actorId: 'admin',
          adminUserIds: ADMIN_IDS,
          userId: 'carol',
          status,
          reason: 'race',
          expectedRevision: 0,
        }),
      ),
    )
    expect(results.map((result) => result.result).sort()).toEqual(['changed', 'conflict'])
    const after = await listAuditLogs({ targetId: 'carol', limit: 100, offset: 0 })
    expect(after.pagination.total).toBe(before + 1)
    const winner = results.find((result) => result.result === 'changed')!
    expect(after.logs[0]!.new_status).toBe(winner.status)
  })

  test('a failed audit write rolls back the status change and session revocation', async () => {
    await createAuthTokens('carol')
    await client.execute(
      `CREATE TRIGGER fail_audit BEFORE INSERT ON moderation_audit_logs BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END`,
    )
    try {
      await expect(
        changeUserModeration({
          actorId: 'admin',
          adminUserIds: ADMIN_IDS,
          userId: 'carol',
          status: 'banned',
          reason: 'x',
          expectedRevision: 0,
        }),
      ).rejects.toThrow()
    } finally {
      await client.execute('DROP TRIGGER fail_audit')
    }
    expect(await db.select().from(schema.userModeration)).toHaveLength(0)
    const sessions = await db
      .select()
      .from(schema.sessions)
      .where(eq(schema.sessions.userId, 'carol'))
    expect(sessions.every((session) => session.revokedAt === null)).toBeTrue()
  })

  test('audit logs cannot be modified or deleted', async () => {
    await setChartStatus(CHART_A, 'under_review')
    await expect(client.execute('DELETE FROM moderation_audit_logs')).rejects.toThrow('append-only')
    await expect(client.execute("UPDATE moderation_audit_logs SET reason = 'x'")).rejects.toThrow(
      'append-only',
    )
  })

  test('search finds users and charts by every supported key without auto-expanding MD5', async () => {
    await submitScore({ id: 'alice' }, submission({ md5: MD5_B }), [], 10)
    await submitScore({ id: 'bob' }, submission({ sha256: CHART_B, md5: MD5_B }), [], 10)
    expect((await searchUsers({ search: 'ALIC', limit: 10, offset: 0 })).users[0]!.id).toBe('alice')
    expect((await searchUsers({ search: 'bob@example', limit: 10, offset: 0 })).users[0]!.id).toBe(
      'bob',
    )
    expect((await searchUsers({ search: 'carol', limit: 10, offset: 0 })).users[0]!.email).toBe(
      'carol@example.invalid',
    )

    const byMd5 = await searchCharts({ search: MD5_B, field: 'md5', limit: 10, offset: 0 })
    expect(byMd5.charts.map((chart) => chart.sha256).sort()).toEqual([CHART_A, CHART_B])
    const byTitle = await searchCharts({ search: 'Blocked', field: 'title', limit: 10, offset: 0 })
    expect(
      byTitle.charts.map((chart) => [chart.sha256, chart.player_count, chart.score_count]),
    ).toEqual([[CHART_B, 1, 1]])
    // 1件だけブロックしても同じ MD5 の別 SHA-256 は変わらない。
    await setChartStatus(CHART_B, 'blocked')
    const blocked = await searchCharts({
      search: '',
      field: 'any',
      status: 'blocked',
      limit: 10,
      offset: 0,
    })
    expect(blocked.charts.map((chart) => chart.sha256)).toEqual([CHART_B])
    const active = await searchCharts({
      search: MD5_B,
      field: 'md5',
      status: 'active',
      limit: 10,
      offset: 0,
    })
    expect(active.charts.map((chart) => chart.sha256)).toEqual([CHART_A])
    const unregistered = await searchCharts({
      search: 'd'.repeat(64),
      field: 'sha256',
      limit: 10,
      offset: 0,
    })
    expect(unregistered.charts).toMatchObject([
      { sha256: 'd'.repeat(64), registered: false, revision: 0 },
    ])
  })
})
