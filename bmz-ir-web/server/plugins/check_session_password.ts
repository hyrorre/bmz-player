/**
 * 本番デプロイで NUXT_SESSION_PASSWORD が未設定のまま起動しないようにする。
 *
 * nuxt.config.ts の runtimeConfig.session.password は空文字 fallback のため、
 * secret を設定し忘れると cookie セッションの暗号化が弱化/失敗したまま
 * 公開されてしまう。dev では nuxt-auth-utils が自動生成するので許容する。
 */
export default defineNitroPlugin(() => {
  if (import.meta.dev) {
    return
  }
  const password = useRuntimeConfig().session?.password ?? ''
  if (typeof password !== 'string' || password.length < 32) {
    throw new Error(
      'NUXT_SESSION_PASSWORD must be set to a random string of at least 32 characters in production',
    )
  }
})
