import { createError } from 'h3'
import { ModerationAdminError } from './admin'

/** 管理者APIの service エラーを HTTP エラーに変換して実行する。 */
export async function handleModerationAdmin<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run()
  } catch (error) {
    if (error instanceof ModerationAdminError) {
      throw createError({
        statusCode: error.statusCode,
        statusMessage: error.message,
        data: error.data,
      })
    }
    throw error
  }
}
