export type Setup = 'stream' | 'code'

export interface SetupsState {
  /** The setup currently shown. */
  selected: Setup
  /** The setup last chosen by tap: restored on launch, returned to when a stream ends. */
  resting: Setup
  streaming: boolean
}

export const DEFAULT_SETUP: Setup = 'code'

export function isSetup(value: unknown): value is Setup {
  return value === 'stream' || value === 'code'
}

export function initialState(resting: Setup): SetupsState {
  return { selected: resting, resting, streaming: false }
}

/** A tap is always respected, including during a stream. */
export function tap(state: SetupsState, setup: Setup): SetupsState {
  return { ...state, selected: setup, resting: setup }
}

/** Acts only when `streaming` changes; anything else returns the same state. */
export function obs(state: SetupsState, streaming: boolean): SetupsState {
  if (streaming === state.streaming) return state
  return { ...state, streaming, selected: streaming ? 'stream' : state.resting }
}
