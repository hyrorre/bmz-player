create table public.charts (
  sha256 text primary key,
  md5 text,
  title text not null default '',
  subtitle text,
  genre text,
  artist text,
  subartists text[] not null default '{}',
  mode text not null,
  level numeric,
  total numeric,
  judge_rank numeric,
  min_bpm numeric,
  max_bpm numeric,
  notes integer not null default 0,
  ln_notes integer not null default 0,
  cn_notes integer not null default 0,
  hcn_notes integer not null default 0,
  mine_notes integer not null default 0,
  has_random boolean not null default false,
  has_stop boolean not null default false,
  has_undefined_ln boolean not null default false,
  has_defined_ln boolean not null default false,
  has_defined_cn boolean not null default false,
  has_defined_hcn boolean not null default false,
  has_ln boolean not null default false,
  has_cn boolean not null default false,
  has_hcn boolean not null default false,
  has_mine boolean not null default false,
  source_url text,
  append_url text,
  headers jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint charts_sha256_hex check (sha256 ~ '^[0-9a-f]{64}$'),
  constraint charts_md5_hex check (md5 is null or md5 ~ '^[0-9a-f]{32}$'),
  constraint charts_mode_known check (mode in ('keys_5', 'keys_7', 'keys_10', 'keys_14', 'unknown')),
  constraint charts_counts_non_negative check (
    notes >= 0 and ln_notes >= 0 and cn_notes >= 0 and hcn_notes >= 0 and mine_notes >= 0
  )
);

create index idx_charts_md5 on public.charts(md5) where md5 is not null;
create index idx_charts_title on public.charts using gin (to_tsvector('simple', title));

create trigger set_charts_updated_at
  before update on public.charts
  for each row
  execute function public.set_updated_at();

alter table public.charts enable row level security;

create policy "Charts are publicly readable"
  on public.charts
  for select
  using (true);

create policy "Authenticated users can upsert charts"
  on public.charts
  for insert
  to authenticated
  with check (true);

create policy "Authenticated users can update charts"
  on public.charts
  for update
  to authenticated
  using (true)
  with check (true);

create table public.scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  chart_sha256 text not null references public.charts(sha256) on delete restrict,
  client_name text not null,
  client_version text not null,
  platform text not null,
  play_mode text not null,
  key_mode text not null,
  gauge text not null,
  ln_policy text not null,
  effective_ln_mode text not null,
  judge_algorithm text not null,
  scoring text not null,
  clear_type text not null,
  clear_rank integer not null,
  played_at timestamptz,
  server_received_at timestamptz not null default now(),
  duration_ms integer,
  judges jsonb not null,
  ex_score integer not null,
  avg_judge_ms numeric,
  max_combo integer not null,
  notes integer not null,
  pass_notes integer not null,
  bp integer not null,
  cb integer not null,
  min_bp integer not null,
  min_cb integer not null,
  play_options jsonb not null default '{}'::jsonb,
  replay_hash text,
  replay_format text,
  replay_upload_intent text,
  evidence jsonb not null default '{}'::jsonb,
  verification text not null default 'unverified',
  accepted boolean not null default true,
  rejection_reason text,
  idempotency_key text not null,
  created_at timestamptz not null default now(),
  constraint scores_chart_sha256_hex check (chart_sha256 ~ '^[0-9a-f]{64}$'),
  constraint scores_ln_policy_known check (
    ln_policy in ('AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn')
  ),
  constraint scores_effective_ln_mode_known check (effective_ln_mode in ('ln', 'cn', 'hcn')),
  constraint scores_scoring_known check (scoring = 'bms_ex_score_v1'),
  constraint scores_clear_rank_non_negative check (clear_rank >= 0),
  constraint scores_counts_non_negative check (
    ex_score >= 0 and max_combo >= 0 and notes >= 0 and pass_notes >= 0
    and bp >= 0 and cb >= 0 and min_bp >= 0 and min_cb >= 0
  ),
  constraint scores_replay_hash_hex check (replay_hash is null or replay_hash ~ '^[0-9a-f]{64}$'),
  constraint scores_verification_known check (
    verification in ('unverified', 'signed', 'invalid', 'trusted')
  ),
  unique (player_id, idempotency_key)
);

create index idx_scores_player_chart_rule
  on public.scores(player_id, chart_sha256, gauge, ln_policy, scoring)
  where accepted = true;
create index idx_scores_chart_rule
  on public.scores(chart_sha256, gauge, ln_policy, scoring)
  where accepted = true;
create index idx_scores_received_at on public.scores(server_received_at desc);

alter table public.scores enable row level security;

create policy "Users can read their own score history"
  on public.scores
  for select
  using ((select auth.uid()) = player_id);

create policy "Users can insert their own score history"
  on public.scores
  for insert
  with check ((select auth.uid()) = player_id);

create table public.best_scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  chart_sha256 text not null references public.charts(sha256) on delete restrict,
  score_id uuid not null references public.scores(id) on delete cascade,
  ex_score integer not null,
  clear_type text not null,
  clear_rank integer not null,
  max_combo integer not null,
  min_bp integer not null,
  min_cb integer not null,
  gauge text not null,
  ln_policy text not null,
  effective_ln_mode text not null,
  scoring text not null,
  played_at timestamptz,
  server_received_at timestamptz not null,
  verification text not null default 'unverified',
  updated_at timestamptz not null default now(),
  constraint best_scores_ln_policy_known check (
    ln_policy in ('AutoLn', 'AutoCn', 'AutoHcn', 'ForceLn', 'ForceCn', 'ForceHcn')
  ),
  constraint best_scores_effective_ln_mode_known check (effective_ln_mode in ('ln', 'cn', 'hcn')),
  constraint best_scores_scoring_known check (scoring = 'bms_ex_score_v1'),
  constraint best_scores_counts_non_negative check (
    ex_score >= 0 and clear_rank >= 0 and max_combo >= 0 and min_bp >= 0 and min_cb >= 0
  ),
  unique (player_id, chart_sha256, gauge, ln_policy, scoring)
);

create index idx_best_scores_chart_rule_rank
  on public.best_scores(chart_sha256, gauge, ln_policy, scoring, ex_score desc);
create index idx_best_scores_player
  on public.best_scores(player_id);

create trigger set_best_scores_updated_at
  before update on public.best_scores
  for each row
  execute function public.set_updated_at();

alter table public.best_scores enable row level security;

create policy "Best scores are publicly readable"
  on public.best_scores
  for select
  using (true);

create table public.rival_relationships (
  owner_player_id uuid not null references public.profiles(id) on delete cascade,
  target_player_id uuid not null references public.profiles(id) on delete cascade,
  relation_type text not null default 'rival',
  created_at timestamptz not null default now(),
  primary key (owner_player_id, target_player_id, relation_type),
  constraint rival_relationships_no_self check (owner_player_id <> target_player_id),
  constraint rival_relationships_relation_type_known check (relation_type in ('rival'))
);

create index idx_rival_relationships_target
  on public.rival_relationships(target_player_id);

alter table public.rival_relationships enable row level security;

create policy "Users can read their own rival relationships"
  on public.rival_relationships
  for select
  using ((select auth.uid()) = owner_player_id);

create policy "Users can manage their own rival relationships"
  on public.rival_relationships
  for all
  using ((select auth.uid()) = owner_player_id)
  with check ((select auth.uid()) = owner_player_id);

create table public.device_keys (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  public_key text not null,
  algorithm text not null default 'ed25519',
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  constraint device_keys_algorithm_known check (algorithm in ('ed25519'))
);

create index idx_device_keys_player
  on public.device_keys(player_id)
  where revoked_at is null;

alter table public.device_keys enable row level security;

create policy "Users can read their own device keys"
  on public.device_keys
  for select
  using ((select auth.uid()) = player_id);

create policy "Users can manage their own device keys"
  on public.device_keys
  for all
  using ((select auth.uid()) = player_id)
  with check ((select auth.uid()) = player_id);

create table public.replay_objects (
  id uuid primary key default gen_random_uuid(),
  score_id uuid not null references public.scores(id) on delete cascade,
  player_id uuid not null references public.profiles(id) on delete cascade,
  object_path text,
  hash text not null,
  format text not null,
  status text not null default 'metadata_only',
  size_bytes bigint,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint replay_objects_hash_hex check (hash ~ '^[0-9a-f]{64}$'),
  constraint replay_objects_status_known check (
    status in ('metadata_only', 'pending_upload', 'uploaded', 'verified', 'rejected')
  )
);

create index idx_replay_objects_score on public.replay_objects(score_id);
create index idx_replay_objects_player on public.replay_objects(player_id);

create trigger set_replay_objects_updated_at
  before update on public.replay_objects
  for each row
  execute function public.set_updated_at();

alter table public.replay_objects enable row level security;

create policy "Users can read their own replay metadata"
  on public.replay_objects
  for select
  using ((select auth.uid()) = player_id);

create policy "Users can insert their own replay metadata"
  on public.replay_objects
  for insert
  with check ((select auth.uid()) = player_id);
