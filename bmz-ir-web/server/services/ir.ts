export {
  CLEAR_RANK,
  MAX_LOCAL_BACKFILL_DELETE_BATCH_SIZE,
  IrBackfillCleanupError,
  IrEvidenceValidationError,
  IrScoreNotFoundError,
  arrangeOptionsFromPlayOptions,
  asRuleMode,
  isRecord,
  normalizeGaugeName,
  parseRankingQuery,
  parseRankingScope,
  requireFiniteNumber,
  requireHex,
  requireNonNegativeInteger,
  validateScoreAttestation,
  validateScoreSubmission,
  validateSeedOptions,
} from './ir/common'
export type { IrRequestUser, LocalBackfillDeleteResult, RankingQuery } from './ir/common'
export { getRanking } from './ir/ranking'
export { attestScore, deleteLocalBackfillScores, submitScore } from './ir/submission'
export { resolveVerification, stableStringify } from './ir/verification'

import {
  arrangeOptionsFromPlayOptions,
  scoreSubmissionMetadata,
  validateSeedOptions,
} from './ir/common'
import {
  bestRowsFromHistory,
  dedupeBestRowsByPlayer,
  previousRankFromEntries,
  rankingJudges,
  rankRows,
} from './ir/ranking'
import {
  idempotentScoreResponse,
  partitionLocalBackfillRows,
  uniqueBestScoreKeys,
} from './ir/submission'
import { verificationStatusForSignedSubmission } from './ir/verification'

export const __test = {
  arrangeOptionsFromPlayOptions,
  dedupeBestRowsByPlayer,
  bestRowsFromHistory,
  uniqueBestScoreKeys,
  partitionLocalBackfillRows,
  verificationStatusForSignedSubmission,
  idempotentScoreResponse,
  scoreSubmissionMetadata,
  validateSeedOptions,
  rankingJudges,
  rankRows,
  previousRankFromEntries,
}
