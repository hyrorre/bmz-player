import { describe, expect, test } from 'bun:test'
import { DIFFICULTY_TABLE_SOURCES } from '../constants/difficulty_tables'
import {
  __test,
  syncAllowlistedDifficultyTables,
  syncDifficultyTable,
  type FetchedDifficultyTable,
} from './difficulty_tables'

describe('difficulty table parsing', () => {
  test('finds bmstable meta regardless of attribute order, quote style, and case', () => {
    expect(
      __test.findBmstableMeta(
        `<html><head><META content='./header.json?x=1&amp;y=2' NAME='BMSTABLE'></head></html>`,
      ),
    ).toBe('./header.json?x=1&y=2')
  })

  test('parses BOM, multiple data URLs, and mixed numeric/string level order', () => {
    expect(
      __test.parseDifficultyTableHeader(
        `\u{feff}{"name":"Stella","symbol":"st","data_url":["a.json","b.json"],"level_order":[0,"1",2,"???",null]}`,
        'https://example.com/header.json',
      ),
    ).toEqual({
      name: 'Stella',
      symbol: 'st',
      dataUrls: ['a.json', 'b.json'],
      levelOrder: ['0', '1', '2', '???'],
    })
  })

  test('normalizes entry hashes and numeric levels while skipping unusable hashes', () => {
    const entries = __test.parseDifficultyTableEntries(
      `\u{feff}[
        {"level": 1, "md5": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "title": "A"},
        {"level": "2", "sha256": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"},
        {"level": 3, "md5": "short"},
        {"level": {"bad": true}, "md5": "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC"}
      ]`,
      'https://example.com/data.json',
    )

    expect(entries).toEqual([
      {
        level: '1',
        md5: 'a'.repeat(32),
        sha256: '',
        title: 'A',
        artist: '',
        comment: '',
      },
      {
        level: '2',
        md5: '',
        sha256: 'b'.repeat(64),
        title: '',
        artist: '',
        comment: '',
      },
    ])
  })
})

describe('difficulty table fetching', () => {
  test('declares the external origins used by the operator allowlist', () => {
    const sources = new Map(
      DIFFICULTY_TABLE_SOURCES.map(({ sourceUrl, allowedOrigins }) => [
        sourceUrl,
        allowedOrigins ?? [],
      ]),
    )

    expect(sources.get('https://rattoto10.jounin.jp/table.html')).toEqual([
      'https://rattoto10.github.io',
    ])
    expect(sources.get('https://mplwtch.github.io/Solomon/')).toEqual([
      'https://script.google.com',
      'https://script.googleusercontent.com',
    ])
    expect(sources.get('https://pmsdifficulty.xxxxxxxx.jp/_pastoral_upper.html')).toEqual([
      'https://pmsdatabase.github.io',
      'https://pmsdatabase-isr.vercel.app',
      'https://script.google.com',
      'https://script.googleusercontent.com',
    ])
    expect(sources.get('https://hibyethere.github.io/table/')).toEqual(['https://asumatoki.kr'])
  })

  test('calls the supplied fetch function without an object receiver', async () => {
    const fetchImpl = function (this: unknown, input: string | URL | Request) {
      expect(this).toBeUndefined()
      const url = String(input)
      const body = url.endsWith('header.json')
        ? `{"name":"X","symbol":"x","data_url":"data.json"}`
        : `[{"level":"1","md5":"${'a'.repeat(32)}"}]`
      return Promise.resolve(new Response(body))
    } as typeof fetch

    const table = await __test.fetchDifficultyTable(
      { sourceUrl: 'https://example.com/header.json' },
      { fetchImpl },
    )
    expect(table.entries).toHaveLength(1)
  })

  test('resolves relative header and multiple data URLs', async () => {
    const requested: string[] = []
    const responses = new Map<string, string>([
      ['https://example.com/table/index.html', `<meta content="./header.json" name="bmstable">`],
      [
        'https://example.com/table/header.json',
        `\u{feff}{"name":"Example","symbol":"★","data_url":["data-a.json","../data-b.json"],"level_order":[1]}`,
      ],
      [
        'https://example.com/table/data-a.json',
        `[{"level":1,"md5":"${'a'.repeat(32)}","title":"A"}]`,
      ],
      [
        'https://example.com/data-b.json',
        `[{"level":"2","sha256":"${'b'.repeat(64)}","title":"B"}]`,
      ],
    ])
    const fetchImpl = (async (input: string | URL | Request) => {
      const url = String(input)
      requested.push(url)
      const body = responses.get(url)
      return body === undefined ? new Response('', { status: 404 }) : new Response(body)
    }) as typeof fetch

    const table = await __test.fetchDifficultyTable(
      { sourceUrl: 'https://example.com/table/index.html' },
      { fetchImpl, now: () => new Date('2026-07-13T00:00:00.000Z') },
    )

    expect(requested).toEqual([...responses.keys()])
    expect(table).toMatchObject({
      headUrl: 'https://example.com/table/header.json',
      name: 'Example',
      symbol: '★',
      levelOrder: ['1', '2'],
      fetchedAt: new Date('2026-07-13T00:00:00.000Z'),
    })
    expect(table.entries).toHaveLength(2)
  })

  test('rejects an undeclared cross-origin data URL before requesting it', async () => {
    const requested: string[] = []
    const fetchImpl = (async (input: string | URL | Request) => {
      const url = String(input)
      requested.push(url)
      return new Response(`{"name":"X","symbol":"x","data_url":"https://other.example/data.json"}`)
    }) as typeof fetch

    await expect(
      __test.fetchDifficultyTable({ sourceUrl: 'https://example.com/header.json' }, { fetchImpl }),
    ).rejects.toThrow('resource origin is not allowlisted')
    expect(requested).toEqual(['https://example.com/header.json'])
  })

  test('follows redirects across explicitly allowed resource origins', async () => {
    const requested: string[] = []
    const fetchImpl = (async (input: string | URL | Request) => {
      const url = String(input)
      requested.push(url)
      if (url === 'https://table.example/header.json') {
        return new Response(`{"name":"X","symbol":"x","data_url":"https://script.google.com/data"}`)
      }
      if (url === 'https://script.google.com/data') {
        return new Response('', {
          status: 302,
          headers: { location: 'https://script.googleusercontent.com/data' },
        })
      }
      return new Response(`[{"level":"1","md5":"${'a'.repeat(32)}"}]`)
    }) as typeof fetch

    const table = await __test.fetchDifficultyTable(
      {
        sourceUrl: 'https://table.example/header.json',
        allowedOrigins: ['https://script.google.com', 'https://script.googleusercontent.com'],
      },
      { fetchImpl },
    )

    expect(requested).toEqual([
      'https://table.example/header.json',
      'https://script.google.com/data',
      'https://script.googleusercontent.com/data',
    ])
    expect(table.entries).toHaveLength(1)
  })

  test('enforces the streaming response size limit', async () => {
    const fetchImpl = (async () => new Response('123456789')) as typeof fetch
    await expect(
      __test.fetchDifficultyTable(
        { sourceUrl: 'https://example.com/header.json' },
        { fetchImpl, maxResponseBytes: 8 },
      ),
    ).rejects.toThrow('exceeds 8 bytes')
  })
})

describe('difficulty table synchronization', () => {
  test('rejects non-allowlisted sources before fetching', async () => {
    let fetched = false
    const fetchImpl = (async () => {
      fetched = true
      return new Response('{}')
    }) as typeof fetch

    await expect(
      syncDifficultyTable('https://other.example/header.json', [], { fetchImpl }),
    ).rejects.toThrow('not allowlisted')
    expect(fetched).toBe(false)
  })

  test('preserves the underlying runtime fetch cause', async () => {
    const fetchImpl = (async () => {
      throw new TypeError('Illegal invocation')
    }) as typeof fetch
    const source = { sourceUrl: 'https://example.com/header.json' }

    const result = await syncAllowlistedDifficultyTables([source], { fetchImpl })
    expect(result.failed[0]?.error).toContain('difficulty table request failed')
    expect(result.failed[0]?.error).toContain('Illegal invocation')
  })

  test('activates only after all generation entries are staged', async () => {
    const calls: string[] = []
    const source = { sourceUrl: 'https://example.com/header.json', priority: 2 }
    const table = fetchedTable()
    const store = {
      async ensureTable(tableId: string) {
        calls.push(`ensure:${tableId}`)
      },
      async insertEntries(_tableId: string, generation: string) {
        calls.push(`insert:${generation}`)
      },
      async activateGeneration(_tableId: string, generation: string) {
        calls.push(`activate:${generation}`)
      },
    }

    const tableId = __test.difficultyTableId(source.sourceUrl)
    await __test.persistDifficultyTableGeneration(store, tableId, 'generation-1', source, table)
    expect(calls).toEqual([`ensure:${tableId}`, 'insert:generation-1', 'activate:generation-1'])
  })

  test('does not activate a generation when staging fails', async () => {
    const calls: string[] = []
    const store = {
      async ensureTable() {
        calls.push('ensure')
      },
      async insertEntries() {
        calls.push('insert')
        throw new Error('D1 batch failed')
      },
      async activateGeneration() {
        calls.push('activate')
      },
    }

    await expect(
      __test.persistDifficultyTableGeneration(
        store,
        'table-1',
        'generation-1',
        { sourceUrl: 'https://example.com/header.json' },
        fetchedTable(),
      ),
    ).rejects.toThrow('D1 batch failed')
    expect(calls).toEqual(['ensure', 'insert'])
  })
})

describe('difficulty labels', () => {
  test('prefers any MD5 match over SHA-256 and sorts labels stably', () => {
    const sha256 = 'a'.repeat(64)
    const md5 = 'b'.repeat(32)
    const result = __test.difficultyLabelsFromRows(
      [{ sha256, md5 }],
      [
        labelRow({
          table_id: 'sha-only',
          table_name: 'SHA only',
          sha256,
          md5: '',
          priority: 0,
        }),
        labelRow({
          table_id: 'z',
          table_name: 'Zeta',
          md5,
          sha256: '',
          priority: 0,
          level: '2',
          levelOrder: ['1', '2'],
        }),
        labelRow({
          table_id: 'a',
          table_name: 'Alpha',
          md5,
          sha256: '',
          priority: 0,
          level: '1',
          levelOrder: ['1', '2'],
        }),
        labelRow({
          table_id: 'priority',
          table_name: 'Later by name',
          md5,
          sha256: '',
          priority: -1,
          level: '3',
        }),
      ],
    )

    expect(result.get(sha256)).toEqual([
      { table_id: 'priority', table_name: 'Later by name', symbol: '★', level: '3' },
      { table_id: 'a', table_name: 'Alpha', symbol: '★', level: '1' },
      { table_id: 'z', table_name: 'Zeta', symbol: '★', level: '2' },
    ])
  })

  test('falls back to SHA-256 when MD5 has no active match', () => {
    const sha256 = 'a'.repeat(64)
    const result = __test.difficultyLabelsFromRows(
      [{ sha256, md5: 'b'.repeat(32) }],
      [labelRow({ table_id: 'sha', table_name: 'SHA', sha256, md5: '' })],
    )
    expect(result.get(sha256)).toEqual([
      { table_id: 'sha', table_name: 'SHA', symbol: '★', level: '1' },
    ])
  })
})

describe('difficulty table failure diagnostics', () => {
  test('limits visible failures and reports omitted entries', () => {
    const diagnostics = __test.difficultyTableSyncFailureDiagnostics(
      [
        { sourceUrl: 'https://one.example/', error: 'first' },
        { sourceUrl: 'https://two.example/', error: 'second' },
        { sourceUrl: 'https://three.example/', error: 'third' },
      ],
      2,
    )

    expect(diagnostics).toEqual({
      failureCount: 3,
      omittedCount: 1,
      failures: [
        { sourceUrl: 'https://one.example/', error: 'first' },
        { sourceUrl: 'https://two.example/', error: 'second' },
      ],
    })
  })

  test('truncates an unexpectedly large runtime error', () => {
    const diagnostics = __test.difficultyTableSyncFailureDiagnostics([
      { sourceUrl: 'https://example.com/', error: 'x'.repeat(600) },
    ])

    expect(diagnostics.failures[0]?.error).toHaveLength(500)
    expect(diagnostics.failures[0]?.error.endsWith('…')).toBe(true)
  })

  test('normalizes an error cause chain', () => {
    const cause = new TypeError('Illegal\ninvocation')
    const error = new Error('difficulty table request failed', { cause })
    expect(__test.formatErrorWithCauses(error)).toBe(
      'difficulty table request failed <- Illegal invocation',
    )
  })
})

function fetchedTable(): FetchedDifficultyTable {
  return {
    sourceUrl: 'https://example.com/header.json',
    headUrl: 'https://example.com/header.json',
    name: 'Example',
    symbol: '★',
    levelOrder: ['1'],
    entries: [
      {
        level: '1',
        md5: 'a'.repeat(32),
        sha256: '',
        title: 'A',
        artist: '',
        comment: '',
      },
    ],
    fetchedAt: new Date('2026-07-13T00:00:00.000Z'),
  }
}

function labelRow(
  overrides: Partial<{
    table_id: string
    table_name: string
    symbol: string
    priority: number
    levelOrder: string[]
    level: string
    md5: string
    sha256: string
  }>,
) {
  return {
    table_id: 'table',
    table_name: 'Table',
    symbol: '★',
    priority: 0,
    levelOrder: ['1'],
    level: '1',
    md5: '',
    sha256: '',
    ...overrides,
  }
}
