alter table public.scores
  add column device_type text not null default 'keyboard',
  add constraint scores_device_type_known check (device_type in ('keyboard', 'controller'));

alter table public.best_scores
  add column device_type text not null default 'keyboard',
  add constraint best_scores_device_type_known check (device_type in ('keyboard', 'controller'));
