import { expect, test } from 'bun:test'
import {
  formatCourseArrange,
  formatRankingArrange,
  formatScoreClientVersion,
  lnFilterItems,
  rankingScoreDisplay,
  selectedScoreAttempt,
} from '../app/utils/rankingDisplay'
import { listReturnQuery } from '../app/composables/useRoutePagination'
import type { ScoreDetailResponse } from '../app/types/scoreDetails'

const noLn = {
  has_undefined_ln: false,
  has_defined_ln: false,
  has_defined_cn: false,
  has_defined_hcn: false,
  has_defined_hln: false,
}

test('shows AUTO(LN) for mixed defined types and all AUTO policies when undefined notes coexist', () => {
  expect(lnFilterItems(noLn)).toEqual([])
  const forceLabels = ['ALL', 'LN', 'CN', 'HCN', 'HLN']
  expect(lnFilterItems({ ...noLn, cn_notes: 3 }).map(({ label }) => label)).toEqual(forceLabels)
  expect(
    lnFilterItems({ ...noLn, has_defined_cn: true, has_defined_hcn: true }).map(
      ({ label }) => label,
    ),
  ).toEqual([...forceLabels, 'AUTO(LN)'])
  expect(lnFilterItems({ ...noLn, has_defined_cn: true }).map(({ label }) => label)).toEqual(
    forceLabels,
  )
  expect(lnFilterItems({ ...noLn, has_undefined_ln: true }).map(({ label }) => label)).toEqual(
    forceLabels,
  )
  expect(
    lnFilterItems({ ...noLn, has_undefined_ln: true, has_defined_hln: true }).map(
      ({ label }) => label,
    ),
  ).toEqual([...forceLabels, 'AUTO(LN)', 'AUTO(CN)', 'AUTO(HCN)'])
})

test('includes the recorded course policies when chart metadata is incomplete', () => {
  expect(lnFilterItems(noLn, ['ForceCn']).map(({ value }) => value)).toEqual(['ALL', 'ForceCn'])
  expect(
    lnFilterItems({ ...noLn, has_defined_cn: true }, ['AutoHcn', 'ForceCn']).map(
      ({ value }) => value,
    ),
  ).toEqual(['ALL', 'ForceLn', 'ForceCn', 'ForceHcn', 'ForceHln', 'AutoHcn'])
})

test('switching score tabs uses the selected attempt for time, device and signature', () => {
  const ex: ScoreDetailResponse = {
    score: {
      id: 'ex',
      client_name: 'BMZ',
      client_version: '1',
      judges: null,
      device_type: 'keyboard',
      played_at: '2026-11-01T12:00:00Z',
      server_received_at: '2026-11-01T12:01:00Z',
      verification: 'verified_play',
    },
    replay: null,
  }
  const clear: ScoreDetailResponse = {
    ...ex,
    score: {
      ...ex.score,
      id: 'clear',
      device_type: 'controller',
      played_at: null,
      server_received_at: '2026-11-03T15:00:00Z',
      verification: 'signed_backfill',
    },
  }
  expect(selectedScoreAttempt('ex', ex)).toMatchObject({
    metadata: { device: 'keyboard', date: ex.score.played_at },
    verification: { label: 'score.verifiedPlay' },
  })
  expect(selectedScoreAttempt('clear', ex)).toBeNull()
  expect(selectedScoreAttempt('clear', clear)).toMatchObject({
    metadata: { device: 'controller', date: clear.score.server_received_at },
    verification: { label: 'score.signedBackfill' },
  })
  const unsigned: ScoreDetailResponse = {
    ...ex,
    score: { ...ex.score, verification: 'unverified' },
  }
  expect(selectedScoreAttempt('ex', unsigned)?.verification.label).toBe('score.unsigned')
})

test('formats the nearest grade border separately from the achieved grade', () => {
  expect(rankingScoreDisplay(6078, 6828)).toMatchObject({ grade: 'AAA', difference: 'AAA+8' })
  expect(rankingScoreDisplay(6078, 6828)?.rate.toFixed(1)).toBe('89.0')
  expect(rankingScoreDisplay(6762, 6828)).toMatchObject({ grade: 'AAA', difference: 'MAX-66' })
  expect(rankingScoreDisplay(6069, 6828)).toMatchObject({ grade: 'AA', difference: 'AAA-1' })
  expect(rankingScoreDisplay(6828, 6828)).toMatchObject({ grade: 'MAX', difference: 'MAX+0' })
  expect(rankingScoreDisplay(99, null)).toBeNull()
})

test('shows BMZ score versions with the bmz-player name', () => {
  expect(formatScoreClientVersion({ client_name: 'BMZ', client_version: '0.5.4' })).toBe(
    'bmz-player 0.5.4',
  )
  expect(formatScoreClientVersion({ client_name: 'bmz', client_version: '0.5.3' })).toBe(
    'bmz-player 0.5.3',
  )
  expect(formatScoreClientVersion({ client_name: 'Other Player', client_version: '1.0' })).toBe(
    'Other Player 1.0',
  )
  expect(formatScoreClientVersion({ client_name: 'BMZ', client_version: null })).toBe('')
})

test('displays two sides for DP and keeps known course options in chart order', () => {
  const randomMirror = { arrange_1p: 'random', arrange_2p: 'mirror' }
  expect(formatRankingArrange(randomMirror, '7K')).toBe('RANDOM')
  expect(formatRankingArrange({ ...randomMirror, double_option: 'off' }, '5K')).toBe('RANDOM')
  expect(formatRankingArrange({ ...randomMirror, double_option: 'battle' }, '7K')).toBe(
    'RANDOM / MIRROR',
  )
  expect(
    formatRankingArrange({ ...randomMirror, double_option: 'battle_auto_scratch' }, '5K'),
  ).toBe('RANDOM / MIRROR')
  expect(formatRankingArrange(randomMirror, '10k')).toBe('RANDOM / MIRROR')
  expect(formatRankingArrange(randomMirror, '14K')).toBe('RANDOM / MIRROR')
  expect(formatCourseArrange([], [{ mode: '14K' }])).toBe('-')
  expect(
    formatCourseArrange(
      [null, randomMirror, randomMirror, randomMirror],
      [{ mode: '7K' }, { mode: '14K' }, { mode: '7K' }, { mode: '14K' }],
    ),
  ).toBe('RANDOM / MIRROR · RANDOM')
})

test('carries only valid list page and search parameters back from detail', () => {
  expect(listReturnQuery({ page: '2', q: ' ★20 ', rule: 'Dx' })).toEqual({ page: '2', q: '★20' })
  for (const page of [null, '0', '-1', '1.5', '9007199254740991', ['2', '3']]) {
    expect(listReturnQuery({ page }).page).toBeUndefined()
  }
})
