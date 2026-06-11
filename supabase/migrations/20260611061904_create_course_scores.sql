-- コーススコア IR (docs/ir.md §19)。
-- ir_courses: course_hash で識別するコース registry。
-- course_scores: 投稿履歴 (譜面ごとの内訳は jsonb entries)。
-- best_course_scores: ランキング用 best。
create table public.ir_courses (
  course_hash text primary key,
  title text not null default '',
  kind text not null default 'course',
  charts jsonb not null,
  chart_count integer not null,
  constraints jsonb not null default '{}'::jsonb,
  source_url text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint ir_courses_hash_hex check (course_hash ~ '^[0-9a-f]{64}$'),
  constraint ir_courses_kind_known check (kind in ('dan', 'course')),
  constraint ir_courses_chart_count_positive check (chart_count > 0)
);

create trigger set_ir_courses_updated_at
  before update on public.ir_courses
  for each row
  execute function public.set_updated_at();

alter table public.ir_courses enable row level security;

create policy "Anyone can read courses"
  on public.ir_courses
  for select
  using (true);

create table public.course_scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  course_hash text not null references public.ir_courses(course_hash),

  client_name text not null,
  client_version text not null,
  platform text not null,

  gauge text not null,
  ln_policy text not null,
  scoring text not null,

  clear_type text not null,
  clear_rank integer not null,
  course_clear boolean not null,
  course_failed boolean not null,
  played_entries integer not null,
  trophies jsonb not null default '[]'::jsonb,

  ex_score integer not null,
  max_ex_score integer not null,
  max_combo integer not null,
  bp integer not null,
  judges jsonb not null,
  gauge_value numeric not null,

  entries jsonb not null,

  played_at timestamptz,
  server_received_at timestamptz not null default now(),
  device_type text not null,
  evidence jsonb not null default '{}'::jsonb,
  verification text not null default 'unverified',
  accepted boolean not null default true,
  idempotency_key text not null,

  constraint course_scores_verification_known check (
    verification in ('unverified', 'signed', 'invalid', 'trusted')
  ),
  constraint course_scores_device_known check (device_type in ('keyboard', 'controller')),
  constraint course_scores_counts_non_negative check (
    ex_score >= 0 and max_ex_score >= 0 and max_combo >= 0 and bp >= 0 and played_entries >= 0
  ),
  unique (player_id, idempotency_key)
);

create index idx_course_scores_course
  on public.course_scores(course_hash, server_received_at desc);

alter table public.course_scores enable row level security;

create policy "Anyone can read course scores"
  on public.course_scores
  for select
  using (true);

create table public.best_course_scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  course_hash text not null references public.ir_courses(course_hash),
  course_score_id uuid not null references public.course_scores(id),

  ex_score integer not null,
  clear_type text not null,
  clear_rank integer not null,
  course_clear boolean not null,
  max_combo integer not null,
  bp integer not null,
  device_type text not null,

  gauge text not null,
  ln_policy text not null,
  scoring text not null,

  played_at timestamptz,
  server_received_at timestamptz not null,
  verification text not null default 'unverified',

  constraint best_course_scores_verification_known check (
    verification in ('unverified', 'signed', 'invalid', 'trusted')
  ),
  unique (player_id, course_hash, gauge, ln_policy, scoring)
);

create index idx_best_course_scores_ranking
  on public.best_course_scores(course_hash, gauge, ln_policy, scoring, ex_score desc);

alter table public.best_course_scores enable row level security;

create policy "Anyone can read best course scores"
  on public.best_course_scores
  for select
  using (true);
