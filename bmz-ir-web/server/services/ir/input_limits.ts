import type { IrScoreSubmission } from '../../../shared/types/ir'

export const MAX_METADATA_CHARS = 1024
export const MAX_URL_CHARS = 2048
export const MAX_SUBARTISTS = 32
export const MAX_COURSE_CHARTS = 64
// 64 ordinary entry_randomizations already occupy about 10 KiB in BMZ's
// current course payload. Keep those keys and allow room for RANDOM choices.
// Dynamic branch decisions and per-play interpretation evidence share the signed options.
// Keep room for metadata and signature inside the existing 256 KiB request limit.
export const MAX_PLAY_OPTIONS_BYTES = 128 * 1024
export const MAX_EVIDENCE_BYTES = 16 * 1024

export function requireOptionalText(value: unknown, maximum: number, field: string): void {
  if (value == null) return
  if (typeof value !== 'string') throw new Error(`${field} must be a string`)
  if (value.length > maximum) throw new Error(`${field} exceeds ${maximum} characters`)
}

export function requireJsonBytes(value: unknown, maximum: number, field: string): void {
  let json: string | undefined
  try {
    json = JSON.stringify(value)
  } catch {
    throw new Error(`${field} must be valid JSON`)
  }
  if (json === undefined) throw new Error(`${field} must be valid JSON`)
  if (new TextEncoder().encode(json).byteLength > maximum) {
    throw new Error(`${field} exceeds ${maximum} JSON bytes`)
  }
}

export function validateSubmissionJsonLimits(playOptions: unknown, evidence: unknown): void {
  requireJsonBytes(playOptions ?? {}, MAX_PLAY_OPTIONS_BYTES, 'play_options')
  requireJsonBytes(evidence ?? {}, MAX_EVIDENCE_BYTES, 'evidence')
}

export function validateChartInputLimits(chart: IrScoreSubmission['chart']): void {
  if (chart.kaleid != null) {
    requireJsonBytes(chart.kaleid, MAX_EVIDENCE_BYTES, 'chart.kaleid')
    const value = chart.kaleid
    if (!Number.isSafeInteger(value.version) || value.version < 0) {
      throw new Error('chart.kaleid.version must be a nonnegative integer')
    }
    for (const field of ['declared', 'source', 'effective'] as const) {
      const flags = value[field]
      if (
        !flags ||
        typeof flags !== 'object' ||
        Array.isArray(flags) ||
        Object.values(flags).some((flag) => typeof flag !== 'boolean')
      ) {
        throw new Error(`chart.kaleid.${field} must contain boolean flags`)
      }
    }
    if (
      !Array.isArray(value.diagnostics) ||
      value.diagnostics.some((item) => typeof item !== 'string')
    ) {
      throw new Error('chart.kaleid.diagnostics must contain strings')
    }
  }
  for (const field of ['title', 'subtitle', 'artist', 'genre', 'difficulty'] as const) {
    requireOptionalText(chart[field], MAX_METADATA_CHARS, `chart.${field}`)
  }
  if (chart.subartists != null) {
    if (!Array.isArray(chart.subartists) || chart.subartists.length > MAX_SUBARTISTS) {
      throw new Error(`chart.subartists must contain at most ${MAX_SUBARTISTS} entries`)
    }
    for (const artist of chart.subartists) {
      if (typeof artist !== 'string') throw new Error('chart.subartists[] must be a string')
      requireOptionalText(artist, MAX_METADATA_CHARS, 'chart.subartists[]')
    }
  }
  if (chart.urls != null) {
    if (typeof chart.urls !== 'object' || Array.isArray(chart.urls)) {
      throw new Error('chart.urls must be an object')
    }
    for (const value of Object.values(chart.urls)) {
      requireOptionalText(value, MAX_URL_CHARS, 'chart.urls value')
    }
  }
}
