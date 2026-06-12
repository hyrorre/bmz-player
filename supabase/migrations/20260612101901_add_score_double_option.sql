alter table public.scores
  add column double_option text not null default 'off',
  add constraint scores_double_option_known
    check (double_option in ('off', 'battle', 'battle_auto_scratch'));

drop index if exists public.idx_scores_player_chart_rule;
drop index if exists public.idx_scores_chart_rule;

create index idx_scores_player_chart_rule
  on public.scores(player_id, chart_sha256, gauge, ln_policy, scoring, double_option)
  where accepted = true;

create index idx_scores_chart_rule
  on public.scores(chart_sha256, gauge, ln_policy, scoring, double_option)
  where accepted = true;

alter table public.best_scores
  add column double_option text not null default 'off',
  add constraint best_scores_double_option_known
    check (double_option in ('off', 'battle', 'battle_auto_scratch'));

alter table public.best_scores
  drop constraint if exists best_scores_player_id_chart_sha256_gauge_ln_policy_scoring_key;

alter table public.best_scores
  add constraint best_scores_player_chart_rule_double_unique
    unique (player_id, chart_sha256, gauge, ln_policy, scoring, double_option);

drop index if exists public.idx_best_scores_chart_rule_rank;

create index idx_best_scores_chart_rule_rank
  on public.best_scores(chart_sha256, gauge, ln_policy, scoring, double_option, ex_score desc);
