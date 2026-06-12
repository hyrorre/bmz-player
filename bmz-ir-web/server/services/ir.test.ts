import { describe, expect, test } from 'bun:test'
import { stableStringify } from './ir'

describe('stableStringify', () => {
  test('matches JCS number formatting used by Rust IR evidence', () => {
    const value = {
      numbers: [333333333.33333329, 1e30, 4.5, 2e-3, 1e-27, 1e-6, 1e-7, -0],
      chart: {
        total: 160.0,
        bpm: {
          min: 120.0,
          max: 120.5,
        },
      },
    }

    expect(stableStringify(value)).toBe(
      '{"chart":{"bpm":{"max":120.5,"min":120},"total":160},"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27,0.000001,1e-7,0]}',
    )
  })

  test('sorts keys by UTF-16 code units', () => {
    expect(
      stableStringify({
        '\u{e000}': 2,
        '\u{10000}': 1,
      }),
    ).toBe('{"𐀀":1,"":2}')
  })

  test('rejects values outside canonical JSON', () => {
    expect(() => stableStringify(undefined)).toThrow()
    expect(() => stableStringify(Number.NaN)).toThrow()
    expect(() => stableStringify(Number.POSITIVE_INFINITY)).toThrow()
  })
})
