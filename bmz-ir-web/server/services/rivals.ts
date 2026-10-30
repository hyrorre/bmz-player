import { and, asc, desc, eq, sql } from 'drizzle-orm'
import { alias } from 'drizzle-orm/sqlite-core'
import { db, schema } from 'hub:db'
import type { IrRivalComparison, IrRivalsResponse, LnScorePolicy } from '../../shared/types/ir'
import { lookupDifficultyLabels } from './difficulty_tables'
import { publicChartCondition, publicPlayerCondition } from './moderation/visibility'
import {
  buildRivalComparisonRow,
  type RivalComparisonScoreRow,
  type RivalMutation,
} from './rivals_core'

const selfBestScores = alias(schema.bestScores, 'self_best_scores')
const rivalBestScores = alias(schema.bestScores, 'rival_best_scores')

export async function rivalTargetExists(playerId: string): Promise<boolean> {
  const rows = await db
    .select({ id: schema.profiles.id })
    .from(schema.profiles)
    .where(and(eq(schema.profiles.id, playerId), publicPlayerCondition(schema.profiles.id)))
    .limit(1)
  return rows.length > 0
}

export async function applyRivalMutation(ownerPlayerId: string, mutation: RivalMutation) {
  if (mutation.action === 'remove') {
    await db
      .delete(schema.rivalRelationships)
      .where(
        and(
          eq(schema.rivalRelationships.ownerPlayerId, ownerPlayerId),
          eq(schema.rivalRelationships.targetPlayerId, mutation.targetPlayerId),
          eq(schema.rivalRelationships.relationType, 'rival'),
        ),
      )
    return { removed: true }
  }

  await db
    .insert(schema.rivalRelationships)
    .values({
      ownerPlayerId,
      targetPlayerId: mutation.targetPlayerId,
      relationType: 'rival',
    })
    .onConflictDoNothing()
  return { added: true }
}

export async function removeRival(ownerPlayerId: string, targetPlayerId: string) {
  return applyRivalMutation(ownerPlayerId, { targetPlayerId, action: 'remove' })
}

export async function loadRivalLists(playerId: string): Promise<IrRivalsResponse> {
  const [rivals, reverseRivals] = await Promise.all([
    db
      .select({
        player_id: schema.rivalRelationships.targetPlayerId,
        relation_type: schema.rivalRelationships.relationType,
        created_at: schema.rivalRelationships.createdAt,
        display_name: schema.profiles.displayName,
        bio: schema.profiles.bio,
      })
      .from(schema.rivalRelationships)
      .innerJoin(schema.profiles, eq(schema.profiles.id, schema.rivalRelationships.targetPlayerId))
      .where(
        and(
          eq(schema.rivalRelationships.ownerPlayerId, playerId),
          eq(schema.rivalRelationships.relationType, 'rival'),
          publicPlayerCondition(schema.rivalRelationships.targetPlayerId),
        ),
      )
      .orderBy(desc(schema.rivalRelationships.createdAt)),
    db
      .select({
        player_id: schema.rivalRelationships.ownerPlayerId,
        relation_type: schema.rivalRelationships.relationType,
        created_at: schema.rivalRelationships.createdAt,
        display_name: schema.profiles.displayName,
        bio: schema.profiles.bio,
      })
      .from(schema.rivalRelationships)
      .innerJoin(schema.profiles, eq(schema.profiles.id, schema.rivalRelationships.ownerPlayerId))
      .where(
        and(
          eq(schema.rivalRelationships.targetPlayerId, playerId),
          eq(schema.rivalRelationships.relationType, 'rival'),
          publicPlayerCondition(schema.rivalRelationships.ownerPlayerId),
        ),
      )
      .orderBy(desc(schema.rivalRelationships.createdAt)),
  ])

  return {
    rivals: rivals.map(rivalEntry),
    reverse_rivals: reverseRivals.map(rivalEntry),
  }
}

export async function loadRivalComparison(
  ownerPlayerId: string,
  targetPlayerId: string,
  limit: number,
  offset: number,
): Promise<IrRivalComparison | null> {
  const relation = await db
    .select({ targetPlayerId: schema.rivalRelationships.targetPlayerId })
    .from(schema.rivalRelationships)
    .where(
      and(
        eq(schema.rivalRelationships.ownerPlayerId, ownerPlayerId),
        eq(schema.rivalRelationships.targetPlayerId, targetPlayerId),
        eq(schema.rivalRelationships.relationType, 'rival'),
        publicPlayerCondition(schema.rivalRelationships.targetPlayerId),
      ),
    )
    .limit(1)
  if (relation.length === 0) return null

  const [ownerProfile, targetProfile, summaryRows, pagedScores] = await Promise.all([
    loadProfile(ownerPlayerId),
    loadProfile(targetPlayerId),
    loadRivalComparisonSummary(ownerPlayerId, targetPlayerId),
    loadRivalComparisonPage(ownerPlayerId, targetPlayerId, limit, offset),
  ])
  if (!ownerProfile || !targetProfile) return null

  const summaryRow = summaryRows[0]
  const total = Number(summaryRow?.total ?? 0)
  const pagedRows = pagedScores.map((row) => {
    const selfScore = comparisonScore(row)
    return buildRivalComparisonRow(selfScore, {
      ...selfScore,
      scoreId: row.rivalScoreId,
      exScore: row.rivalExScore,
      minBp: row.rivalMinBp,
    })
  })
  const labels = await lookupDifficultyLabels(
    pagedRows.map((row) => ({ sha256: row.chart.sha256, md5: row.chart.md5 })),
  )

  return {
    players: { self: ownerProfile, rival: targetProfile },
    summary: {
      ex_score: {
        wins: Number(summaryRow?.exScoreWins ?? 0),
        losses: Number(summaryRow?.exScoreLosses ?? 0),
        draws: Number(summaryRow?.exScoreDraws ?? 0),
      },
      min_bp: {
        wins: Number(summaryRow?.minBpWins ?? 0),
        losses: Number(summaryRow?.minBpLosses ?? 0),
        draws: Number(summaryRow?.minBpDraws ?? 0),
      },
    },
    comparisons: pagedRows.map((row) => ({
      ...row,
      difficulty_labels: labels.get(row.chart.sha256) ?? [],
    })),
    pagination: {
      limit,
      offset,
      total,
      has_more: offset + limit < total,
    },
  }
}

async function loadProfile(playerId: string) {
  const rows = await db
    .select({ id: schema.profiles.id, display_name: schema.profiles.displayName })
    .from(schema.profiles)
    .where(eq(schema.profiles.id, playerId))
    .limit(1)
  return rows[0] ?? null
}

function matchedRivalScoreIdentity() {
  return and(
    eq(selfBestScores.chartSha256, rivalBestScores.chartSha256),
    eq(selfBestScores.lnPolicy, rivalBestScores.lnPolicy),
    eq(selfBestScores.doubleOption, rivalBestScores.doubleOption),
    eq(selfBestScores.ruleMode, rivalBestScores.ruleMode),
    eq(selfBestScores.scoring, rivalBestScores.scoring),
  )
}

async function loadRivalComparisonSummary(ownerPlayerId: string, targetPlayerId: string) {
  return db
    .select({
      total: sql<number>`count(*)`,
      exScoreWins: sql<number>`coalesce(sum(case when ${selfBestScores.exScore} > ${rivalBestScores.exScore} then 1 else 0 end), 0)`,
      exScoreLosses: sql<number>`coalesce(sum(case when ${selfBestScores.exScore} < ${rivalBestScores.exScore} then 1 else 0 end), 0)`,
      exScoreDraws: sql<number>`coalesce(sum(case when ${selfBestScores.exScore} = ${rivalBestScores.exScore} then 1 else 0 end), 0)`,
      minBpWins: sql<number>`coalesce(sum(case when ${selfBestScores.minBp} < ${rivalBestScores.minBp} then 1 else 0 end), 0)`,
      minBpLosses: sql<number>`coalesce(sum(case when ${selfBestScores.minBp} > ${rivalBestScores.minBp} then 1 else 0 end), 0)`,
      minBpDraws: sql<number>`coalesce(sum(case when ${selfBestScores.minBp} = ${rivalBestScores.minBp} then 1 else 0 end), 0)`,
    })
    .from(selfBestScores)
    .innerJoin(rivalBestScores, matchedRivalScoreIdentity())
    .where(rivalComparisonCondition(ownerPlayerId, targetPlayerId))
}

function rivalComparisonCondition(ownerPlayerId: string, targetPlayerId: string) {
  return and(
    eq(selfBestScores.playerId, ownerPlayerId),
    eq(rivalBestScores.playerId, targetPlayerId),
    // 比較の件数・勝敗と一覧の両方から非公開譜面を除く。
    publicChartCondition(selfBestScores.chartSha256),
  )
}

async function loadRivalComparisonPage(
  ownerPlayerId: string,
  targetPlayerId: string,
  limit: number,
  offset: number,
) {
  return db
    .select({
      selfScoreId: selfBestScores.scoreId,
      rivalScoreId: rivalBestScores.scoreId,
      chartSha256: selfBestScores.chartSha256,
      chartMd5: schema.charts.md5,
      chartTitle: schema.charts.title,
      chartSubtitle: schema.charts.subtitle,
      chartArtist: schema.charts.artist,
      chartMode: schema.charts.mode,
      chartLevel: schema.charts.level,
      chartDifficulty: schema.charts.difficulty,
      lnPolicy: selfBestScores.lnPolicy,
      doubleOption: selfBestScores.doubleOption,
      ruleMode: selfBestScores.ruleMode,
      scoring: selfBestScores.scoring,
      selfExScore: selfBestScores.exScore,
      selfMinBp: selfBestScores.minBp,
      rivalExScore: rivalBestScores.exScore,
      rivalMinBp: rivalBestScores.minBp,
    })
    .from(selfBestScores)
    .innerJoin(rivalBestScores, matchedRivalScoreIdentity())
    .innerJoin(schema.charts, eq(schema.charts.sha256, selfBestScores.chartSha256))
    .where(rivalComparisonCondition(ownerPlayerId, targetPlayerId))
    .orderBy(
      sql`${schema.charts.level} is null`,
      desc(schema.charts.level),
      asc(schema.charts.title),
      asc(selfBestScores.chartSha256),
      asc(selfBestScores.lnPolicy),
      asc(selfBestScores.doubleOption),
      asc(selfBestScores.ruleMode),
      asc(selfBestScores.scoring),
    )
    .limit(limit)
    .offset(offset)
}

function comparisonScore(
  row: Awaited<ReturnType<typeof loadRivalComparisonPage>>[number],
): RivalComparisonScoreRow {
  return {
    scoreId: row.selfScoreId,
    chartSha256: row.chartSha256,
    chartMd5: row.chartMd5,
    chartTitle: row.chartTitle,
    chartSubtitle: row.chartSubtitle,
    chartArtist: row.chartArtist,
    chartMode: row.chartMode,
    chartLevel: row.chartLevel,
    chartDifficulty: row.chartDifficulty,
    lnPolicy: row.lnPolicy as LnScorePolicy,
    doubleOption: row.doubleOption,
    ruleMode: row.ruleMode,
    scoring: row.scoring,
    exScore: row.selfExScore,
    minBp: row.selfMinBp,
  }
}

function rivalEntry(row: {
  player_id: string
  relation_type: 'rival'
  created_at: Date
  display_name: string
  bio: string
}) {
  return {
    player_id: row.player_id,
    relation_type: row.relation_type,
    created_at: row.created_at.toISOString(),
    profile: { id: row.player_id, display_name: row.display_name, bio: row.bio || null },
  }
}
