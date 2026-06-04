---
name: supabase-ir-db
description: Use when changing BMZ IR Supabase schema, migrations, RLS policies, generated database types, or server-side Supabase access code.
---

When working on BMZ IR Supabase database changes:

1. Inspect `supabase/migrations`, `supabase/config.toml`, and `bmz-ir-web/shared/types/database.types.ts` before editing.
2. Treat `supabase/migrations/*.sql` as the source of truth for schema, indexes, constraints, RLS policies, grants, and SQL functions.
3. Create migrations with `bun run db:new <name>` or `supabase migration new <name>`.
4. Do not make production schema changes directly through the dashboard, MCP, or ad hoc SQL. If a remote change already exists, pull it into a migration with `supabase db pull` and review the SQL before committing.
5. Keep generated types in `bmz-ir-web/shared/types/database.types.ts`.
6. Do not put secrets, `sb_secret_...` keys, legacy service role keys, DB passwords, refresh tokens, or production data in repo files.
7. Use `sb_publishable_...` for browser, desktop, and public clients. Put it in `NUXT_PUBLIC_SUPABASE_KEY` / `SUPABASE_PUBLISHABLE_KEY`. Treat legacy `anon` keys as compatibility-only.
8. Use `sb_secret_...` only in server-side code. Put it in `NUXT_SUPABASE_SECRET_KEY` / `SUPABASE_SECRET_KEY`. Treat legacy `service_role` keys as compatibility-only and never expose them through public env vars.
9. Ask explicit user approval before `bun run db:push`, destructive SQL, remote writes, or anything that touches production.
10. After migration changes, run:
   - `bun run db:reset`
   - `bun run db:types`
   - `bun run build`
11. If local Supabase is not running, explain that `bun run db:start` is required before local reset/type generation.
