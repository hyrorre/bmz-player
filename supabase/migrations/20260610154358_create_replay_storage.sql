-- Replay ファイル保存用の private bucket。
-- アクセスはすべて Nitro server route (service role) 経由で行い、
-- クライアントには署名付き upload URL だけを渡すため、storage.objects への
-- 追加 RLS policy は定義しない (service role は RLS をバイパスする)。
insert into storage.buckets (id, name, public)
values ('replays', 'replays', false)
on conflict (id) do nothing;

-- upload-url 発行時に score 単位で upsert するため unique にする。
alter table public.replay_objects
  add constraint replay_objects_score_unique unique (score_id);
