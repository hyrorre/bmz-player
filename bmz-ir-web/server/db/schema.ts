import { sql } from 'drizzle-orm'
import { index, integer, primaryKey, sqliteTable, text, uniqueIndex } from 'drizzle-orm/sqlite-core'

const jsonText = <T>(name: string, fallback: string) =>
  text(name, { mode: 'json' })
    .$type<T>()
    .notNull()
    .default(sql.raw(`'${fallback}'`))

export const users = sqliteTable(
  'users',
  {
    id: text('id').primaryKey(),
    email: text('email').notNull(),
    passwordHash: text('password_hash').notNull(),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [uniqueIndex('idx_users_email').on(table.email)],
)

export const sessions = sqliteTable(
  'sessions',
  {
    tokenHash: text('token_hash').primaryKey(),
    sessionGroupId: text('session_group_id'),
    userId: text('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    kind: text('kind', { enum: ['access', 'refresh'] }).notNull(),
    clientType: text('client_type', { enum: ['web', 'desktop'] })
      .notNull()
      .default('web'),
    expiresAt: integer('expires_at', { mode: 'timestamp_ms' }).notNull(),
    lastUsedAt: integer('last_used_at', { mode: 'timestamp_ms' }),
    revokedAt: integer('revoked_at', { mode: 'timestamp_ms' }),
    revokedReason: text('revoked_reason', {
      enum: ['logout', 'rotated', 'password_changed', 'reuse_detected', 'admin'],
    }),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    index('idx_sessions_user').on(table.userId),
    index('idx_sessions_group').on(table.userId, table.sessionGroupId),
    index('idx_sessions_kind_expiry').on(table.kind, table.expiresAt),
  ],
)

export const authRateLimits = sqliteTable(
  'auth_rate_limits',
  {
    action: text('action', { enum: ['login', 'register'] }).notNull(),
    scope: text('scope', { enum: ['email', 'ip'] }).notNull(),
    scopeHash: text('scope_hash').notNull(),
    windowStart: integer('window_start', { mode: 'timestamp_ms' }).notNull(),
    attempts: integer('attempts').notNull().default(0),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    primaryKey({ columns: [table.action, table.scope, table.scopeHash, table.windowStart] }),
    index('idx_auth_rate_limits_updated_at').on(table.updatedAt),
  ],
)

export const profiles = sqliteTable('profiles', {
  id: text('id')
    .primaryKey()
    .references(() => users.id, { onDelete: 'cascade' }),
  displayName: text('display_name').notNull().default(''),
  bio: text('bio').notNull().default(''),
  createdAt: integer('created_at', { mode: 'timestamp_ms' })
    .notNull()
    .default(sql`(unixepoch('subsec') * 1000)`),
  updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
    .notNull()
    .default(sql`(unixepoch('subsec') * 1000)`),
})

export const charts = sqliteTable(
  'charts',
  {
    sha256: text('sha256').primaryKey(),
    md5: text('md5'),
    title: text('title').notNull().default(''),
    subtitle: text('subtitle'),
    genre: text('genre'),
    artist: text('artist'),
    subartists: jsonText<string[]>('subartists', '[]'),
    mode: text('mode').notNull(),
    level: integer('level'),
    total: integer('total'),
    judgeRank: integer('judge_rank'),
    minBpm: integer('min_bpm'),
    maxBpm: integer('max_bpm'),
    notes: integer('notes').notNull().default(0),
    lnNotes: integer('ln_notes').notNull().default(0),
    cnNotes: integer('cn_notes').notNull().default(0),
    hcnNotes: integer('hcn_notes').notNull().default(0),
    mineNotes: integer('mine_notes').notNull().default(0),
    hasRandom: integer('has_random', { mode: 'boolean' }).notNull().default(false),
    hasStop: integer('has_stop', { mode: 'boolean' }).notNull().default(false),
    hasUndefinedLn: integer('has_undefined_ln', { mode: 'boolean' }).notNull().default(false),
    hasDefinedLn: integer('has_defined_ln', { mode: 'boolean' }).notNull().default(false),
    hasDefinedCn: integer('has_defined_cn', { mode: 'boolean' }).notNull().default(false),
    hasDefinedHcn: integer('has_defined_hcn', { mode: 'boolean' }).notNull().default(false),
    hasLn: integer('has_ln', { mode: 'boolean' }).notNull().default(false),
    hasCn: integer('has_cn', { mode: 'boolean' }).notNull().default(false),
    hasHcn: integer('has_hcn', { mode: 'boolean' }).notNull().default(false),
    hasMine: integer('has_mine', { mode: 'boolean' }).notNull().default(false),
    sourceUrl: text('source_url'),
    appendUrl: text('append_url'),
    headers: jsonText<Record<string, string>>('headers', '{}'),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [index('idx_charts_md5').on(table.md5), index('idx_charts_title').on(table.title)],
)

export const scores = sqliteTable(
  'scores',
  {
    id: text('id').primaryKey(),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    chartSha256: text('chart_sha256')
      .notNull()
      .references(() => charts.sha256, { onDelete: 'restrict' }),
    clientName: text('client_name').notNull(),
    clientVersion: text('client_version').notNull(),
    platform: text('platform').notNull(),
    playMode: text('play_mode').notNull(),
    keyMode: text('key_mode').notNull(),
    gauge: text('gauge').notNull(),
    lnPolicy: text('ln_policy').notNull(),
    effectiveLnMode: text('effective_ln_mode').notNull(),
    judgeAlgorithm: text('judge_algorithm').notNull(),
    scoring: text('scoring').notNull(),
    clearType: text('clear_type').notNull(),
    clearRank: integer('clear_rank').notNull(),
    playedAt: integer('played_at', { mode: 'timestamp_ms' }),
    serverReceivedAt: integer('server_received_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
    durationMs: integer('duration_ms'),
    judges: jsonText<Record<string, unknown>>('judges', '{}'),
    exScore: integer('ex_score').notNull(),
    avgJudgeMs: integer('avg_judge_ms'),
    maxCombo: integer('max_combo').notNull(),
    notes: integer('notes').notNull(),
    passNotes: integer('pass_notes').notNull(),
    bp: integer('bp').notNull(),
    cb: integer('cb').notNull(),
    minBp: integer('min_bp').notNull(),
    minCb: integer('min_cb').notNull(),
    deviceType: text('device_type', { enum: ['keyboard', 'controller'] })
      .notNull()
      .default('keyboard'),
    doubleOption: text('double_option', { enum: ['off', 'battle', 'battle_auto_scratch'] })
      .notNull()
      .default('off'),
    playOptions: jsonText<Record<string, unknown>>('play_options', '{}'),
    replayHash: text('replay_hash'),
    replayFormat: text('replay_format'),
    replayUploadIntent: text('replay_upload_intent'),
    evidence: jsonText<Record<string, unknown>>('evidence', '{}'),
    verification: text('verification', { enum: ['unverified', 'signed', 'invalid', 'trusted'] })
      .notNull()
      .default('unverified'),
    accepted: integer('accepted', { mode: 'boolean' }).notNull().default(true),
    rejectionReason: text('rejection_reason'),
    idempotencyKey: text('idempotency_key').notNull(),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    uniqueIndex('idx_scores_player_idempotency').on(table.playerId, table.idempotencyKey),
    index('idx_scores_player_chart_rule').on(
      table.playerId,
      table.chartSha256,
      table.gauge,
      table.lnPolicy,
      table.scoring,
      table.doubleOption,
    ),
    index('idx_scores_chart_rule').on(
      table.chartSha256,
      table.gauge,
      table.lnPolicy,
      table.scoring,
      table.doubleOption,
    ),
    index('idx_scores_received_at').on(table.serverReceivedAt),
  ],
)

export const bestScores = sqliteTable(
  'best_scores',
  {
    id: text('id').primaryKey(),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    chartSha256: text('chart_sha256')
      .notNull()
      .references(() => charts.sha256, { onDelete: 'restrict' }),
    scoreId: text('score_id')
      .notNull()
      .references(() => scores.id, { onDelete: 'cascade' }),
    exScore: integer('ex_score').notNull(),
    clearType: text('clear_type').notNull(),
    clearRank: integer('clear_rank').notNull(),
    maxCombo: integer('max_combo').notNull(),
    minBp: integer('min_bp').notNull(),
    minCb: integer('min_cb').notNull(),
    deviceType: text('device_type', { enum: ['keyboard', 'controller'] })
      .notNull()
      .default('keyboard'),
    doubleOption: text('double_option', { enum: ['off', 'battle', 'battle_auto_scratch'] })
      .notNull()
      .default('off'),
    gauge: text('gauge').notNull(),
    lnPolicy: text('ln_policy').notNull(),
    effectiveLnMode: text('effective_ln_mode').notNull(),
    scoring: text('scoring').notNull(),
    playedAt: integer('played_at', { mode: 'timestamp_ms' }),
    serverReceivedAt: integer('server_received_at', { mode: 'timestamp_ms' }).notNull(),
    verification: text('verification', { enum: ['unverified', 'signed', 'invalid', 'trusted'] })
      .notNull()
      .default('unverified'),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    uniqueIndex('idx_best_scores_player_chart_rule').on(
      table.playerId,
      table.chartSha256,
      table.gauge,
      table.lnPolicy,
      table.scoring,
      table.doubleOption,
    ),
    index('idx_best_scores_chart_rule_rank').on(
      table.chartSha256,
      table.gauge,
      table.lnPolicy,
      table.scoring,
      table.doubleOption,
      table.exScore,
    ),
    index('idx_best_scores_player').on(table.playerId),
  ],
)

export const rivalRelationships = sqliteTable(
  'rival_relationships',
  {
    ownerPlayerId: text('owner_player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    targetPlayerId: text('target_player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    relationType: text('relation_type', { enum: ['rival'] })
      .notNull()
      .default('rival'),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    primaryKey({ columns: [table.ownerPlayerId, table.targetPlayerId, table.relationType] }),
    index('idx_rival_relationships_target').on(table.targetPlayerId),
  ],
)

export const deviceKeys = sqliteTable(
  'device_keys',
  {
    id: text('id').primaryKey(),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    publicKey: text('public_key').notNull(),
    algorithm: text('algorithm', { enum: ['ed25519'] })
      .notNull()
      .default('ed25519'),
    revokedAt: integer('revoked_at', { mode: 'timestamp_ms' }),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [index('idx_device_keys_player').on(table.playerId)],
)

export const replayObjects = sqliteTable(
  'replay_objects',
  {
    id: text('id').primaryKey(),
    scoreId: text('score_id')
      .notNull()
      .references(() => scores.id, { onDelete: 'cascade' }),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    objectPath: text('object_path'),
    hash: text('hash').notNull(),
    format: text('format').notNull(),
    status: text('status', {
      enum: ['metadata_only', 'pending_upload', 'uploaded', 'verified', 'rejected'],
    })
      .notNull()
      .default('metadata_only'),
    sizeBytes: integer('size_bytes'),
    createdAt: integer('created_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
    updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
  },
  (table) => [
    uniqueIndex('idx_replay_objects_score').on(table.scoreId),
    index('idx_replay_objects_player').on(table.playerId),
  ],
)

export const irCourses = sqliteTable('ir_courses', {
  courseHash: text('course_hash').primaryKey(),
  title: text('title').notNull().default(''),
  kind: text('kind', { enum: ['dan', 'course'] })
    .notNull()
    .default('course'),
  charts: jsonText<string[]>('charts', '[]'),
  chartCount: integer('chart_count').notNull(),
  constraints: jsonText<Record<string, unknown>>('constraints', '{}'),
  sourceUrl: text('source_url'),
  createdAt: integer('created_at', { mode: 'timestamp_ms' })
    .notNull()
    .default(sql`(unixepoch('subsec') * 1000)`),
  updatedAt: integer('updated_at', { mode: 'timestamp_ms' })
    .notNull()
    .default(sql`(unixepoch('subsec') * 1000)`),
})

export const courseScores = sqliteTable(
  'course_scores',
  {
    id: text('id').primaryKey(),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    courseHash: text('course_hash')
      .notNull()
      .references(() => irCourses.courseHash),
    clientName: text('client_name').notNull(),
    clientVersion: text('client_version').notNull(),
    platform: text('platform').notNull(),
    gauge: text('gauge').notNull(),
    lnPolicy: text('ln_policy').notNull(),
    scoring: text('scoring').notNull(),
    clearType: text('clear_type').notNull(),
    clearRank: integer('clear_rank').notNull(),
    courseClear: integer('course_clear', { mode: 'boolean' }).notNull(),
    courseFailed: integer('course_failed', { mode: 'boolean' }).notNull(),
    playedEntries: integer('played_entries').notNull(),
    trophies: jsonText<string[]>('trophies', '[]'),
    exScore: integer('ex_score').notNull(),
    maxExScore: integer('max_ex_score').notNull(),
    maxCombo: integer('max_combo').notNull(),
    bp: integer('bp').notNull(),
    judges: jsonText<Record<string, unknown>>('judges', '{}'),
    gaugeValue: integer('gauge_value').notNull(),
    entries: jsonText<Record<string, unknown>[]>('entries', '[]'),
    playedAt: integer('played_at', { mode: 'timestamp_ms' }),
    serverReceivedAt: integer('server_received_at', { mode: 'timestamp_ms' })
      .notNull()
      .default(sql`(unixepoch('subsec') * 1000)`),
    deviceType: text('device_type', { enum: ['keyboard', 'controller'] }).notNull(),
    evidence: jsonText<Record<string, unknown>>('evidence', '{}'),
    verification: text('verification', { enum: ['unverified', 'signed', 'invalid', 'trusted'] })
      .notNull()
      .default('unverified'),
    accepted: integer('accepted', { mode: 'boolean' }).notNull().default(true),
    idempotencyKey: text('idempotency_key').notNull(),
  },
  (table) => [
    uniqueIndex('idx_course_scores_player_idempotency').on(table.playerId, table.idempotencyKey),
    index('idx_course_scores_course').on(table.courseHash, table.serverReceivedAt),
  ],
)

export const bestCourseScores = sqliteTable(
  'best_course_scores',
  {
    id: text('id').primaryKey(),
    playerId: text('player_id')
      .notNull()
      .references(() => profiles.id, { onDelete: 'cascade' }),
    courseHash: text('course_hash')
      .notNull()
      .references(() => irCourses.courseHash),
    courseScoreId: text('course_score_id')
      .notNull()
      .references(() => courseScores.id),
    exScore: integer('ex_score').notNull(),
    clearType: text('clear_type').notNull(),
    clearRank: integer('clear_rank').notNull(),
    courseClear: integer('course_clear', { mode: 'boolean' }).notNull(),
    maxCombo: integer('max_combo').notNull(),
    bp: integer('bp').notNull(),
    deviceType: text('device_type', { enum: ['keyboard', 'controller'] }).notNull(),
    gauge: text('gauge').notNull(),
    lnPolicy: text('ln_policy').notNull(),
    scoring: text('scoring').notNull(),
    playedAt: integer('played_at', { mode: 'timestamp_ms' }),
    serverReceivedAt: integer('server_received_at', { mode: 'timestamp_ms' }).notNull(),
    verification: text('verification', { enum: ['unverified', 'signed', 'invalid', 'trusted'] })
      .notNull()
      .default('unverified'),
  },
  (table) => [
    uniqueIndex('idx_best_course_scores_player_course_rule').on(
      table.playerId,
      table.courseHash,
      table.gauge,
      table.lnPolicy,
      table.scoring,
    ),
    index('idx_best_course_scores_ranking').on(
      table.courseHash,
      table.gauge,
      table.lnPolicy,
      table.scoring,
      table.exScore,
    ),
  ],
)
