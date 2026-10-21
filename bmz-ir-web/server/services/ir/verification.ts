import { createHash, createPublicKey, verify as cryptoVerify } from 'node:crypto'
import { and, eq, isNull } from 'drizzle-orm'
import { db, schema } from 'hub:db'
import type {
  IrAppliedDoubleOption,
  IrChartLnProfile,
  IrDeviceType,
  IrDoubleOption,
  IrJudgeCounts,
  IrJudges,
  IrRanking,
  IrRankingEntry,
  IrRankingScope,
  IrRuleMode,
  IrScoreSubmission,
  IrScoreSourceKind,
  IrSubmitResponse,
  IrVerificationStatus,
  LnScorePolicy,
} from '../../../shared/types/ir'

import { IrEvidenceValidationError, LOCAL_BACKFILL_SOURCE, isRecord } from './common'

export async function resolveVerification(
  playerIdOrDb: string | unknown,
  payloadOrPlayerId:
    { evidence?: Record<string, unknown>; play_options?: Record<string, unknown> } | string,
  maybePayload?: { evidence?: Record<string, unknown>; play_options?: Record<string, unknown> },
): Promise<IrVerificationStatus> {
  const playerId = typeof playerIdOrDb === 'string' ? playerIdOrDb : String(payloadOrPlayerId)
  const payload =
    typeof playerIdOrDb === 'string'
      ? (payloadOrPlayerId as {
          evidence?: Record<string, unknown>
          play_options?: Record<string, unknown>
        })
      : (maybePayload ?? {})
  const evidence = payload.evidence
  if (!evidence || typeof evidence !== 'object') {
    return 'unverified'
  }
  const signature = evidence.client_signature
  const keyId = evidence.public_key_id
  const claimedHash = evidence.canonical_hash
  if (!signature) {
    return 'unverified'
  }
  if (
    typeof signature !== 'string' ||
    typeof keyId !== 'string' ||
    typeof claimedHash !== 'string'
  ) {
    throw new IrEvidenceValidationError('score evidence is invalid')
  }

  const key = await db.query.deviceKeys.findFirst({
    columns: { publicKey: true },
    where: and(
      eq(schema.deviceKeys.id, keyId),
      eq(schema.deviceKeys.playerId, playerId),
      isNull(schema.deviceKeys.revokedAt),
    ),
  })
  if (!key) {
    throw new IrEvidenceValidationError('score evidence is invalid')
  }

  const hash = createHash('sha256').update(canonicalSubmissionJson(payload)).digest()
  if (hash.toString('hex') !== claimedHash.toLowerCase()) {
    throw new IrEvidenceValidationError('score evidence is invalid')
  }

  try {
    // Ed25519 raw public key (32 bytes) を SPKI DER に包んで検証する。
    const der = Buffer.concat([
      Buffer.from('302a300506032b6570032100', 'hex'),
      Buffer.from(key.publicKey, 'hex'),
    ])
    const publicKey = createPublicKey({ key: der, format: 'der', type: 'spki' })
    const signatureBytes = Buffer.from(signature, 'base64url')
    if (!cryptoVerify(null, hash, publicKey, signatureBytes)) {
      throw new IrEvidenceValidationError('score evidence is invalid')
    }
    return verificationStatusForSignedSubmission(payload)
  } catch {
    throw new IrEvidenceValidationError('score evidence is invalid')
  }
}

export function verificationStatusForSignedSubmission(payload: {
  play_options?: Record<string, unknown>
}): IrVerificationStatus {
  return payload.play_options?.submission_source === LOCAL_BACKFILL_SOURCE
    ? 'signed_backfill'
    : 'verified_play'
}

export function canonicalSubmissionJson(payload: { evidence?: Record<string, unknown> }): string {
  const clone: Record<string, unknown> = { ...payload }
  delete clone.evidence
  return stableStringify(clone)
}

/** キー昇順・空白なしの決定的 JSON 文字列化。 */
export function stableStringify(value: unknown): string {
  if (value === undefined) {
    throw new Error('canonical JSON does not support undefined')
  }
  if (typeof value === 'number' && !Number.isFinite(value)) {
    throw new Error('canonical JSON number must be finite')
  }
  if (value === null || typeof value !== 'object') {
    const serialized = JSON.stringify(value)
    if (serialized === undefined) {
      throw new Error('canonical JSON value is not serializable')
    }
    return serialized
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(',')}]`
  }
  const record = value as Record<string, unknown>
  const parts = Object.keys(record)
    .filter((key) => record[key] !== undefined)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`)
  return `{${parts.join(',')}}`
}

/** played_at は ISO 文字列または unix 秒 (BMZ client) を受け付ける。 */
