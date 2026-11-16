import { describe, expect, it } from 'vitest'
import { DEFAULT_SETUP, initialState, isSetup, obs, tap, type SetupsState } from './setups'

describe('setups reducer', () => {
  it('defaults to code, not streaming', () => {
    expect(DEFAULT_SETUP).toBe('code')
    expect(initialState('code')).toEqual({ selected: 'code', resting: 'code', streaming: false })
    expect(initialState('stream')).toEqual({
      selected: 'stream',
      resting: 'stream',
      streaming: false,
    })
  })

  it('isSetup accepts only known setups', () => {
    expect(isSetup('stream')).toBe(true)
    expect(isSetup('code')).toBe(true)
    expect(isSetup('other')).toBe(false)
    expect(isSetup(null)).toBe(false)
  })

  it('tap sets selected and resting', () => {
    expect(tap(initialState('code'), 'stream')).toEqual({
      selected: 'stream',
      resting: 'stream',
      streaming: false,
    })
  })

  it('stream start switches to stream, end returns to resting', () => {
    const live = obs(initialState('code'), true)
    expect(live).toEqual({ selected: 'stream', resting: 'code', streaming: true })
    expect(obs(live, false)).toEqual({ selected: 'code', resting: 'code', streaming: false })
  })

  it('repeat events are no-ops returning the same object', () => {
    const idle = initialState('code')
    expect(obs(idle, false)).toBe(idle)
    const live = obs(idle, true)
    expect(obs(live, true)).toBe(live)
  })

  it('tap code mid-stream stays code, and after the stream ends is code', () => {
    let s: SetupsState = obs(initialState('code'), true)
    s = tap(s, 'code')
    expect(s.selected).toBe('code')
    s = obs(s, false)
    expect(s.selected).toBe('code')
  })

  it('tap stream mid-stream makes stream the resting setup', () => {
    let s: SetupsState = obs(initialState('code'), true)
    s = tap(s, 'stream')
    s = obs(s, false)
    expect(s).toEqual({ selected: 'stream', resting: 'stream', streaming: false })
  })

  it('a tap during a stream is not overridden by a repeat stream event', () => {
    let s: SetupsState = obs(initialState('code'), true)
    s = tap(s, 'code')
    expect(obs(s, true).selected).toBe('code')
  })
})
