import { createHash, randomUUID } from 'node:crypto'
import type { BatchItem } from 'drizzle-orm/batch'
import { and, eq, inArray, ne, or } from 'drizzle-orm'
import { db, schema } from 'hub:db'

const DEFAULT_TIMEOUT_MS = 15_000
const DEFAULT_MAX_RESPONSE_BYTES = 8 * 1024 * 1024
const DEFAULT_INSERT_BATCH_SIZE = 250
// A lookup uses up to two bind parameters (MD5 and SHA-256) per chart. Keep the
// total comfortably below D1's per-query bind limit used elsewhere in this app.
export const DIFFICULTY_LABEL_LOOKUP_CHUNK_SIZE = 40
const MAX_REDIRECTS = 5
const FAILURE_LOG_LIMIT = 10
const FAILURE_ERROR_MAX_LENGTH = 500

export interface DifficultyTableSource {
  sourceUrl: string
  /**
   * Header/data JSON may be hosted separately from the table page. Every
   * cross-origin target must be declared here; the source origin is implicit.
   */
  allowedOrigins?: readonly string[]
  /** Slow dynamic data endpoints may opt into a longer per-request timeout. */
  requestTimeoutMs?: number
  priority?: number
}

export interface FetchedDifficultyTableEntry {
  level: string
  md5: string
  sha256: string
  title: string
  artist: string
  comment: string
}

export interface FetchedDifficultyTable {
  sourceUrl: string
  headUrl: string
  name: string
  symbol: string
  levelOrder: string[]
  entries: FetchedDifficultyTableEntry[]
  fetchedAt: Date
}

export interface DifficultyTableSyncResult {
  tableId: string
  sourceUrl: string
  generation: string
  entryCount: number
  fetchedAt: string
}

export interface DifficultyTableSyncFailure {
  sourceUrl: string
  error: string
}

export interface DifficultyTableSyncBatchResult {
  successful: DifficultyTableSyncResult[]
  failed: DifficultyTableSyncFailure[]
}

export interface DifficultyTableSyncFailureDiagnostics {
  failureCount: number
  omittedCount: number
  failures: DifficultyTableSyncFailure[]
}

interface DifficultyTableFetchOptions {
  fetchImpl?: typeof fetch
  timeoutMs?: number
  maxResponseBytes?: number
  now?: () => Date
}

interface DifficultyTableSyncOptions extends DifficultyTableFetchOptions {
  generation?: () => string
  insertBatchSize?: number
  store?: DifficultyTableGenerationStore
}

interface DifficultyTableGenerationStore {
  ensureTable(
    tableId: string,
    source: DifficultyTableSource,
    table: FetchedDifficultyTable,
  ): Promise<void>
  insertEntries(
    tableId: string,
    generation: string,
    entries: readonly FetchedDifficultyTableEntry[],
  ): Promise<void>
  activateGeneration(
    tableId: string,
    generation: string,
    source: DifficultyTableSource,
    table: FetchedDifficultyTable,
  ): Promise<void>
}

export interface DifficultyLabelChart {
  sha256: string
  md5?: string | null
}

export interface DifficultyTableLabel {
  table_id: string
  table_name: string
  symbol: string
  level: string
}

interface DifficultyLabelRow extends DifficultyTableLabel {
  priority: number
  levelOrder: string[]
  md5: string
  sha256: string
}

interface ParsedDifficultyTableHeader {
  name: string
  symbol: string
  dataUrls: string[]
  levelOrder: string[]
}

/**
 * Synchronizes one operator-allowlisted table. Arbitrary URLs are rejected
 * before any network request is made.
 */
export async function syncDifficultyTable(
  sourceUrl: string,
  allowlist: readonly DifficultyTableSource[],
  options: DifficultyTableSyncOptions = {},
): Promise<DifficultyTableSyncResult> {
  const normalizedSourceUrl = normalizeHttpUrl(sourceUrl)
  const source = allowlist.find(
    (candidate) => normalizeHttpUrl(candidate.sourceUrl) === normalizedSourceUrl,
  )
  if (!source) {
    throw new Error(`difficulty table is not allowlisted: ${sourceUrl}`)
  }

  const normalizedSource: DifficultyTableSource = {
    ...source,
    sourceUrl: normalizedSourceUrl,
  }
  const table = await fetchDifficultyTable(normalizedSource, options)
  if (table.entries.length === 0) {
    throw new Error(`difficulty table has no valid entries: ${table.headUrl}`)
  }

  const tableId = difficultyTableId(normalizedSourceUrl)
  const generation = (options.generation ?? randomUUID)()
  const store = options.store ?? createDrizzleDifficultyTableStore(options.insertBatchSize)
  await persistDifficultyTableGeneration(store, tableId, generation, normalizedSource, table)

  return {
    tableId,
    sourceUrl: table.sourceUrl,
    generation,
    entryCount: table.entries.length,
    fetchedAt: table.fetchedAt.toISOString(),
  }
}

/**
 * Looks up labels from active table generations in bounded queries.
 *
 * The returned map is keyed by normalized SHA-256. For each chart, any active
 * MD5 match takes precedence over all SHA-256 matches, matching the desktop
 * difficulty-table importer.
 */
export async function lookupDifficultyLabels(
  charts: readonly DifficultyLabelChart[],
): Promise<Map<string, DifficultyTableLabel[]>> {
  const normalized = charts.map((chart) => ({
    sha256: chart.sha256.toLowerCase(),
    md5: chart.md5?.toLowerCase() || null,
  }))
  const rows: DifficultyLabelRow[] = []

  for (let offset = 0; offset < normalized.length; offset += DIFFICULTY_LABEL_LOOKUP_CHUNK_SIZE) {
    const chunk = normalized.slice(offset, offset + DIFFICULTY_LABEL_LOOKUP_CHUNK_SIZE)
    const md5s = [...new Set(chunk.flatMap((chart) => (chart.md5 ? [chart.md5] : [])))]
    const sha256s = [...new Set(chunk.flatMap((chart) => (chart.sha256 ? [chart.sha256] : [])))]
    const hashCondition =
      md5s.length > 0 && sha256s.length > 0
        ? or(
            inArray(schema.difficultyTableEntries.md5, md5s),
            inArray(schema.difficultyTableEntries.sha256, sha256s),
          )
        : md5s.length > 0
          ? inArray(schema.difficultyTableEntries.md5, md5s)
          : sha256s.length > 0
            ? inArray(schema.difficultyTableEntries.sha256, sha256s)
            : undefined
    if (!hashCondition) continue

    const chunkRows = await db
      .select({
        table_id: schema.difficultyTables.id,
        table_name: schema.difficultyTables.name,
        symbol: schema.difficultyTables.symbol,
        priority: schema.difficultyTables.priority,
        levelOrder: schema.difficultyTables.levelOrder,
        level: schema.difficultyTableEntries.level,
        md5: schema.difficultyTableEntries.md5,
        sha256: schema.difficultyTableEntries.sha256,
      })
      .from(schema.difficultyTableEntries)
      .innerJoin(
        schema.difficultyTables,
        and(
          eq(schema.difficultyTables.id, schema.difficultyTableEntries.tableId),
          eq(schema.difficultyTables.activeGeneration, schema.difficultyTableEntries.generation),
        ),
      )
      .where(hashCondition)

    rows.push(
      ...chunkRows.map((row) => ({
        ...row,
        table_id: String(row.table_id),
      })),
    )
  }

  return difficultyLabelsFromRows(normalized, rows)
}

/** Synchronizes every configured table without letting one failure stop the rest. */
export async function syncAllowlistedDifficultyTables(
  allowlist: readonly DifficultyTableSource[],
  options: DifficultyTableSyncOptions = {},
): Promise<DifficultyTableSyncBatchResult> {
  const successful: DifficultyTableSyncResult[] = []
  const failed: DifficultyTableSyncFailure[] = []

  for (const source of allowlist) {
    try {
      successful.push(await syncDifficultyTable(source.sourceUrl, allowlist, options))
    } catch (error) {
      failed.push({
        sourceUrl: source.sourceUrl,
        error: formatErrorWithCauses(error),
      })
    }
  }

  return { successful, failed }
}

/** Cloudflare Logs向けに件数とエラー文字列の長さを制限した失敗詳細を作る。 */
export function difficultyTableSyncFailureDiagnostics(
  failures: readonly DifficultyTableSyncFailure[],
  limit = FAILURE_LOG_LIMIT,
): DifficultyTableSyncFailureDiagnostics {
  const normalizedLimit = Number.isSafeInteger(limit) && limit > 0 ? limit : FAILURE_LOG_LIMIT
  const visible = failures.slice(0, normalizedLimit).map((failure) => ({
    sourceUrl: failure.sourceUrl,
    error: truncateDiagnostic(failure.error, FAILURE_ERROR_MAX_LENGTH),
  }))
  return {
    failureCount: failures.length,
    omittedCount: Math.max(0, failures.length - visible.length),
    failures: visible,
  }
}

async function fetchDifficultyTable(
  source: DifficultyTableSource,
  options: DifficultyTableFetchOptions,
): Promise<FetchedDifficultyTable> {
  const fetchOptions: Required<Omit<DifficultyTableFetchOptions, 'now'>> = {
    fetchImpl: options.fetchImpl ?? fetch,
    timeoutMs: difficultyTableRequestTimeoutMs(source, options.timeoutMs),
    maxResponseBytes: options.maxResponseBytes ?? DEFAULT_MAX_RESPONSE_BYTES,
  }
  if (!Number.isSafeInteger(fetchOptions.timeoutMs) || fetchOptions.timeoutMs <= 0) {
    throw new Error('difficulty table timeoutMs must be a positive integer')
  }
  if (!Number.isSafeInteger(fetchOptions.maxResponseBytes) || fetchOptions.maxResponseBytes <= 0) {
    throw new Error('difficulty table maxResponseBytes must be a positive integer')
  }

  const allowedOrigins = allowedResourceOrigins(source)
  const sourceUrl = normalizeHttpUrl(source.sourceUrl)
  let headUrl = sourceUrl

  if (!isJsonUrl(sourceUrl)) {
    const html = await fetchText(sourceUrl, allowedOrigins, fetchOptions)
    const headerReference = findBmstableMeta(html)
    if (!headerReference) {
      throw new Error(`no <meta name="bmstable"> at ${sourceUrl}`)
    }
    headUrl = resolveAllowedUrl(sourceUrl, headerReference, allowedOrigins)
  }

  const headerText = await fetchText(headUrl, allowedOrigins, fetchOptions)
  const header = parseDifficultyTableHeader(headerText, headUrl)
  if (header.dataUrls.length === 0) {
    throw new Error(`difficulty table header has no data_url: ${headUrl}`)
  }

  const entries: FetchedDifficultyTableEntry[] = []
  const levelOrder = [...header.levelOrder]
  for (const dataReference of header.dataUrls) {
    const dataUrl = resolveAllowedUrl(headUrl, dataReference, allowedOrigins)
    const dataText = await fetchText(dataUrl, allowedOrigins, fetchOptions)
    const dataEntries = parseDifficultyTableEntries(dataText, dataUrl)
    for (const entry of dataEntries) {
      if (!levelOrder.includes(entry.level)) levelOrder.push(entry.level)
      entries.push(entry)
    }
  }

  return {
    sourceUrl,
    headUrl,
    name: header.name,
    symbol: header.symbol,
    levelOrder,
    entries,
    fetchedAt: (options.now ?? (() => new Date()))(),
  }
}

function difficultyTableRequestTimeoutMs(
  source: DifficultyTableSource,
  requestedTimeoutMs?: number,
): number {
  return requestedTimeoutMs ?? source.requestTimeoutMs ?? DEFAULT_TIMEOUT_MS
}

async function persistDifficultyTableGeneration(
  store: DifficultyTableGenerationStore,
  tableId: string,
  generation: string,
  source: DifficultyTableSource,
  table: FetchedDifficultyTable,
): Promise<string> {
  await store.ensureTable(tableId, source, table)
  await store.insertEntries(tableId, generation, table.entries)
  // This is deliberately last. A fetch or staging failure leaves the previous
  // generation active and readers never observe a partially inserted one.
  await store.activateGeneration(tableId, generation, source, table)
  return tableId
}

function createDrizzleDifficultyTableStore(
  requestedBatchSize = DEFAULT_INSERT_BATCH_SIZE,
): DifficultyTableGenerationStore {
  if (!Number.isSafeInteger(requestedBatchSize) || requestedBatchSize <= 0) {
    throw new Error('difficulty table insertBatchSize must be a positive integer')
  }

  return {
    async ensureTable(tableId, source, table) {
      await db
        .insert(schema.difficultyTables)
        .values({
          id: tableId,
          sourceUrl: table.sourceUrl,
          headUrl: table.headUrl,
          name: table.name,
          symbol: table.symbol,
          levelOrder: table.levelOrder,
          priority: source.priority ?? 0,
          activeGeneration: null,
          lastFetchedAt: null,
          updatedAt: table.fetchedAt,
        })
        .onConflictDoNothing({ target: schema.difficultyTables.id })
    },

    async insertEntries(tableId, generation, entries) {
      for (let offset = 0; offset < entries.length; offset += requestedBatchSize) {
        const chunk = entries.slice(offset, offset + requestedBatchSize)
        const statements = chunk.map((entry) =>
          db.insert(schema.difficultyTableEntries).values({
            tableId,
            generation,
            level: entry.level,
            md5: entry.md5,
            sha256: entry.sha256,
            title: entry.title,
            artist: entry.artist,
            comment: entry.comment,
          }),
        ) as unknown as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]]
        await db.batch(statements)
      }
    },

    async activateGeneration(tableId, generation, source, table) {
      // D1 batch is atomic. Keeping the active pointer update in its own final
      // batch makes the generation visible in one step after all rows exist.
      await db.batch([
        db
          .update(schema.difficultyTables)
          .set({
            headUrl: table.headUrl,
            name: table.name,
            symbol: table.symbol,
            levelOrder: table.levelOrder,
            priority: source.priority ?? 0,
            activeGeneration: generation,
            lastFetchedAt: table.fetchedAt,
            updatedAt: table.fetchedAt,
          })
          .where(eq(schema.difficultyTables.id, tableId)),
        db
          .delete(schema.difficultyTableEntries)
          .where(
            and(
              eq(schema.difficultyTableEntries.tableId, tableId),
              ne(schema.difficultyTableEntries.generation, generation),
            ),
          ),
      ])
    },
  }
}

function parseDifficultyTableHeader(body: string, url: string): ParsedDifficultyTableHeader {
  const value = parseJson(stripBom(body), url)
  if (!isRecord(value)) throw new Error(`difficulty table header is not an object: ${url}`)
  if (typeof value.name !== 'string') {
    throw new Error(`difficulty table header name is missing: ${url}`)
  }
  if (typeof value.symbol !== 'string') {
    throw new Error(`difficulty table header symbol is missing: ${url}`)
  }

  let dataUrls: string[]
  if (value.data_url === undefined || value.data_url === null) {
    dataUrls = []
  } else if (typeof value.data_url === 'string') {
    dataUrls = [value.data_url]
  } else if (
    Array.isArray(value.data_url) &&
    value.data_url.every((item) => typeof item === 'string')
  ) {
    dataUrls = value.data_url
  } else {
    throw new Error(`difficulty table header data_url is invalid: ${url}`)
  }

  const levelOrder = Array.isArray(value.level_order)
    ? value.level_order.flatMap((level) => {
        const normalized = normalizeLevel(level)
        return normalized === null ? [] : [normalized]
      })
    : []

  return { name: value.name, symbol: value.symbol, dataUrls, levelOrder }
}

function parseDifficultyTableEntries(body: string, url: string): FetchedDifficultyTableEntry[] {
  const value = parseJson(stripBom(body), url)
  if (!Array.isArray(value)) {
    throw new Error(`difficulty table data is not an array: ${url}`)
  }

  const entries: FetchedDifficultyTableEntry[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const level = normalizeLevel(item.level)
    if (level === null) continue
    const md5 = optionalString(item.md5).toLowerCase()
    const sha256 = optionalString(item.sha256).toLowerCase()
    // Match the desktop importer: accept an entry when either hash has a
    // plausible digest length, then let lookup prefer MD5 and fall back to SHA-256.
    if (md5.length < 24 && sha256.length < 24) continue

    entries.push({
      level,
      md5,
      sha256,
      title: optionalString(item.title),
      artist: optionalString(item.artist),
      comment: optionalString(item.comment),
    })
  }
  return entries
}

async function fetchText(
  url: string,
  allowedOrigins: ReadonlySet<string>,
  options: Required<Omit<DifficultyTableFetchOptions, 'now'>>,
): Promise<string> {
  let currentUrl = assertAllowedResourceUrl(url, allowedOrigins)

  for (let redirects = 0; redirects <= MAX_REDIRECTS; redirects += 1) {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), options.timeoutMs)
    let response: Response | undefined
    try {
      // workerdのglobal fetchはreceiverに依存するため、optionsのメソッドとして
      // 呼ばず、先にローカル変数へ取り出してreceiverなしで呼び出す。
      const fetchImpl = options.fetchImpl
      response = await fetchImpl(currentUrl, {
        redirect: 'manual',
        signal: controller.signal,
        headers: { 'user-agent': 'bmz-ir/0.1' },
      })

      if (response.status >= 300 && response.status < 400) {
        const location = response.headers.get('location')
        if (!location) throw new Error(`difficulty table redirect has no location: ${currentUrl}`)
        if (redirects === MAX_REDIRECTS) {
          throw new Error(`difficulty table has too many redirects: ${url}`)
        }
        currentUrl = resolveAllowedUrl(currentUrl, location, allowedOrigins)
        continue
      }

      if (!response.ok) {
        throw new Error(`difficulty table request returned HTTP ${response.status}: ${currentUrl}`)
      }
      // Keep the abort timer alive while consuming the body as well as while
      // waiting for response headers.
      return await readResponseText(response, currentUrl, options.maxResponseBytes)
    } catch (error) {
      if (controller.signal.aborted) {
        throw new Error(`difficulty table request timed out: ${currentUrl}`)
      }
      if (!response) {
        throw new Error(`difficulty table request failed: ${currentUrl}`, { cause: error })
      }
      throw error
    } finally {
      clearTimeout(timeout)
    }
  }

  throw new Error(`difficulty table has too many redirects: ${url}`)
}

async function readResponseText(
  response: Response,
  url: string,
  maxBytes: number,
): Promise<string> {
  const contentLength = response.headers.get('content-length')
  if (contentLength !== null) {
    const parsed = Number(contentLength)
    if (Number.isFinite(parsed) && parsed > maxBytes) {
      throw new Error(`difficulty table response exceeds ${maxBytes} bytes: ${url}`)
    }
  }

  if (!response.body) return ''
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let byteLength = 0
  let text = ''
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      byteLength += value.byteLength
      if (byteLength > maxBytes) {
        await reader.cancel()
        throw new Error(`difficulty table response exceeds ${maxBytes} bytes: ${url}`)
      }
      text += decoder.decode(value, { stream: true })
    }
    return text + decoder.decode()
  } finally {
    reader.releaseLock()
  }
}

function allowedResourceOrigins(source: DifficultyTableSource): ReadonlySet<string> {
  const sourceUrl = new URL(normalizeHttpUrl(source.sourceUrl))
  const origins = new Set([sourceUrl.origin])
  for (const configured of source.allowedOrigins ?? []) {
    const url = new URL(configured)
    if (url.protocol !== 'http:' && url.protocol !== 'https:') {
      throw new Error(`difficulty table allowed origin must use HTTP(S): ${configured}`)
    }
    if (url.username || url.password) {
      throw new Error(`difficulty table allowed origin must not contain credentials: ${configured}`)
    }
    origins.add(url.origin)
  }
  return origins
}

function resolveAllowedUrl(
  base: string,
  reference: string,
  allowedOrigins: ReadonlySet<string>,
): string {
  return assertAllowedResourceUrl(new URL(reference, base).href, allowedOrigins)
}

function assertAllowedResourceUrl(url: string, allowedOrigins: ReadonlySet<string>): string {
  const parsed = new URL(url)
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`difficulty table URL must use HTTP(S): ${url}`)
  }
  if (parsed.username || parsed.password) {
    throw new Error(`difficulty table URL must not contain credentials: ${url}`)
  }
  if (!allowedOrigins.has(parsed.origin)) {
    throw new Error(`difficulty table resource origin is not allowlisted: ${parsed.origin}`)
  }
  parsed.hash = ''
  return parsed.href
}

function normalizeHttpUrl(url: string): string {
  const parsed = new URL(url)
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`difficulty table source must use HTTP(S): ${url}`)
  }
  if (parsed.username || parsed.password) {
    throw new Error(`difficulty table source must not contain credentials: ${url}`)
  }
  parsed.hash = ''
  return parsed.href
}

function findBmstableMeta(html: string): string | null {
  for (const tag of html.matchAll(/<meta\b[^>]*>/giu)) {
    const attrs = parseHtmlAttributes(tag[0])
    if (attrs.get('name')?.toLowerCase() === 'bmstable') {
      const content = attrs.get('content')
      if (content) return decodeHtmlAttribute(content)
    }
  }
  return null
}

function parseHtmlAttributes(tag: string): Map<string, string> {
  const attributes = new Map<string, string>()
  const attributePattern = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/gu
  for (const match of tag.matchAll(attributePattern)) {
    const name = match[1]?.toLowerCase()
    if (!name || name === '<meta') continue
    attributes.set(name, match[2] ?? match[3] ?? match[4] ?? '')
  }
  return attributes
}

function decodeHtmlAttribute(value: string): string {
  return value
    .replace(/&#(x[0-9a-f]+|\d+);/giu, (_, code: string) => {
      const numeric =
        code[0]?.toLowerCase() === 'x' ? Number.parseInt(code.slice(1), 16) : Number(code)
      return Number.isFinite(numeric) ? String.fromCodePoint(numeric) : _
    })
    .replace(/&quot;/giu, '"')
    .replace(/&#39;|&apos;/giu, "'")
    .replace(/&lt;/giu, '<')
    .replace(/&gt;/giu, '>')
    .replace(/&amp;/giu, '&')
}

function isJsonUrl(url: string): boolean {
  return new URL(url).pathname.toLowerCase().endsWith('.json')
}

function stripBom(value: string): string {
  return value.startsWith('\u{feff}') ? value.slice(1) : value
}

function parseJson(value: string, url: string): unknown {
  try {
    return JSON.parse(value)
  } catch (error) {
    throw new Error(`invalid difficulty table JSON: ${url}`, { cause: error })
  }
}

function normalizeLevel(value: unknown): string | null {
  if (typeof value === 'string') return value
  if (typeof value === 'number' && Number.isFinite(value)) return String(value)
  return null
}

function optionalString(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function difficultyTableId(sourceUrl: string): string {
  return `table_${createHash('sha256').update(sourceUrl).digest('hex').slice(0, 32)}`
}

function truncateDiagnostic(value: string, maxLength: number): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 1)}…`
}

function formatErrorWithCauses(error: unknown): string {
  const messages: string[] = []
  const seen = new Set<unknown>()
  let current: unknown = error

  for (let depth = 0; current !== undefined && current !== null && depth < 4; depth += 1) {
    if (seen.has(current)) break
    seen.add(current)
    const message = normalizeDiagnosticText(
      current instanceof Error ? current.message : String(current),
    )
    if (message && messages.at(-1) !== message) messages.push(message)
    current = current instanceof Error ? current.cause : undefined
  }

  return messages.join(' <- ') || 'unknown error'
}

function normalizeDiagnosticText(value: string): string {
  return value
    .replace(/[\r\n\t]+/gu, ' ')
    .replace(/\s{2,}/gu, ' ')
    .trim()
}

function difficultyLabelsFromRows(
  charts: readonly { sha256: string; md5: string | null }[],
  rows: readonly DifficultyLabelRow[],
): Map<string, DifficultyTableLabel[]> {
  const byMd5 = groupDifficultyRows(rows, (row) => row.md5)
  const bySha256 = groupDifficultyRows(rows, (row) => row.sha256)
  const result = new Map<string, DifficultyTableLabel[]>()

  for (const chart of charts) {
    const md5Matches = chart.md5 ? (byMd5.get(chart.md5) ?? []) : []
    const matches = md5Matches.length > 0 ? md5Matches : (bySha256.get(chart.sha256) ?? [])
    const seen = new Set<string>()
    const labels = [...matches].sort(compareDifficultyRows).flatMap((row) => {
      const key = `${row.table_id}\u0000${row.level}`
      if (seen.has(key)) return []
      seen.add(key)
      return [
        {
          table_id: row.table_id,
          table_name: row.table_name,
          symbol: row.symbol,
          level: row.level,
        },
      ]
    })
    result.set(chart.sha256, labels)
  }

  return result
}

function groupDifficultyRows(
  rows: readonly DifficultyLabelRow[],
  key: (row: DifficultyLabelRow) => string,
): Map<string, DifficultyLabelRow[]> {
  const groups = new Map<string, DifficultyLabelRow[]>()
  for (const row of rows) {
    const value = key(row)
    if (!value) continue
    const group = groups.get(value) ?? []
    group.push(row)
    groups.set(value, group)
  }
  return groups
}

function compareDifficultyRows(left: DifficultyLabelRow, right: DifficultyLabelRow): number {
  const priority = left.priority - right.priority
  if (priority !== 0) return priority
  const name = left.table_name.localeCompare(right.table_name)
  if (name !== 0) return name
  const leftLevel = left.levelOrder.indexOf(left.level)
  const rightLevel = right.levelOrder.indexOf(right.level)
  const levelOrder =
    (leftLevel < 0 ? Number.MAX_SAFE_INTEGER : leftLevel) -
    (rightLevel < 0 ? Number.MAX_SAFE_INTEGER : rightLevel)
  return levelOrder || left.level.localeCompare(right.level)
}

export const __test = {
  findBmstableMeta,
  parseDifficultyTableHeader,
  parseDifficultyTableEntries,
  fetchDifficultyTable,
  persistDifficultyTableGeneration,
  difficultyTableId,
  difficultyLabelsFromRows,
  difficultyTableSyncFailureDiagnostics,
  difficultyTableRequestTimeoutMs,
  formatErrorWithCauses,
}
