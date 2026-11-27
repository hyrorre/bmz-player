import { describe, expect, test } from 'bun:test'
import { parseRivalMutation } from './rivals_core'

describe('rival mutation input', () => {
  test('accepts explicit add and remove actions', () => {
    expect(parseRivalMutation({ target_player_id: ' rival ', action: 'add' }, 'self')).toEqual({
      targetPlayerId: 'rival',
      action: 'add',
    })
    expect(parseRivalMutation({ target_player_id: 'rival', action: 'remove' }, 'self')).toEqual({
      targetPlayerId: 'rival',
      action: 'remove',
    })
  })

  test('rejects self rivals and implicit actions', () => {
    expect(() => parseRivalMutation({ target_player_id: 'self', action: 'add' }, 'self')).toThrow(
      'valid target_player_id is required',
    )
    expect(() => parseRivalMutation({ target_player_id: 'rival' }, 'self')).toThrow(
      'action must be add or remove',
    )
  })
})
