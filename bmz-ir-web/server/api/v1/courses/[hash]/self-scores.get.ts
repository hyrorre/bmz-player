import { and, desc, eq, sql } from 'drizzle-orm'
import { getQuery } from 'h3'
import { db, schema } from 'hub:db'
import { asRuleMode, normalizeGaugeName, requireHex } from '../../../../services/ir'
import { requireIrUser } from '../../../../utils/auth'

export default defineEventHandler(async (event) => {
  const courseHash = getRouterParam(event, 'hash')
  if (!courseHash) {
    throw createError({ statusCode: 400, statusMessage: 'course hash is required' })
  }
  requireHex(courseHash, 64, 'course hash')

  const user = await requireIrUser(event)
  const query = getQuery(event)
  const gauge =
    typeof query.gauge === 'string' && query.gauge && query.gauge !== 'ALL'
      ? normalizeGaugeName(query.gauge)
      : null
  const lnPolicy =
    typeof query.ln_policy === 'string' && query.ln_policy && query.ln_policy !== 'ALL'
      ? query.ln_policy
      : null
  const ruleMode =
    typeof query.rule_mode === 'string' && query.rule_mode && query.rule_mode !== 'ALL'
      ? asRuleMode(query.rule_mode)
      : null
  const limit = Math.max(1, Math.min(100, Number(query.limit ?? 50) || 50))
  const offset = Math.max(0, Number(query.offset ?? 0) || 0)
  const conditions = [
    eq(schema.courseScores.playerId, user.id),
    eq(schema.courseScores.courseHash, courseHash),
    eq(schema.courseScores.accepted, true),
    eq(schema.courseScores.scoring, 'bms_ex_score_v1'),
  ]
  if (gauge) {
    conditions.push(eq(schema.courseScores.gauge, gauge))
  }
  if (lnPolicy) {
    conditions.push(eq(schema.courseScores.lnPolicy, lnPolicy))
  }
  if (ruleMode) {
    conditions.push(eq(schema.courseScores.ruleMode, ruleMode))
  }

  const [countRows, rows] = await Promise.all([
    db
      .select({ total: sql<number>`count(*)` })
      .from(schema.courseScores)
      .where(and(...conditions)),
    db
      .select({
        course_score_id: schema.courseScores.id,
        clear: schema.courseScores.clearType,
        course_clear: schema.courseScores.courseClear,
        ex_score: schema.courseScores.exScore,
        max_combo: schema.courseScores.maxCombo,
        bp: schema.courseScores.bp,
        gauge: schema.courseScores.gauge,
        ln_policy: schema.courseScores.lnPolicy,
        rule_mode: schema.courseScores.ruleMode,
        device_type: schema.courseScores.deviceType,
        played_at: schema.courseScores.playedAt,
        server_received_at: schema.courseScores.serverReceivedAt,
        verification: schema.courseScores.verification,
      })
      .from(schema.courseScores)
      .where(and(...conditions))
      .orderBy(desc(schema.courseScores.serverReceivedAt))
      .limit(limit)
      .offset(offset),
  ])
  const total = Number(countRows[0]?.total ?? 0)

  return {
    course: { course_hash: courseHash },
    rule: { gauge, ln_policy: lnPolicy, rule_mode: ruleMode, scoring: 'bms_ex_score_v1' },
    scores: rows.map((row) => ({
      ...row,
      played_at: row.played_at?.toISOString() ?? null,
      server_received_at: row.server_received_at.toISOString(),
    })),
    pagination: {
      limit,
      offset,
      total,
      has_more: offset + limit < total,
    },
  }
})
