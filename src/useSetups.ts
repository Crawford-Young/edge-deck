import { useCallback, useEffect, useReducer } from 'react'
import { obsState, onObsState } from './api'
import {
  DEFAULT_SETUP,
  initialState,
  isSetup,
  obs,
  tap,
  type Setup,
  type SetupsState,
} from './setups'

const STORAGE_KEY = 'edge-deck.setup'

type Action = { type: 'tap'; setup: Setup } | { type: 'obs'; streaming: boolean }

function reduce(state: SetupsState, action: Action): SetupsState {
  return action.type === 'tap' ? tap(state, action.setup) : obs(state, action.streaming)
}

function readResting(): Setup {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    return isSetup(stored) ? stored : DEFAULT_SETUP
  } catch {
    return DEFAULT_SETUP
  }
}

export function useSetups(): { selected: Setup; select: (setup: Setup) => void } {
  const [state, dispatch] = useReducer(reduce, undefined, () => initialState(readResting()))

  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined
    const apply = (streaming: boolean) => {
      if (active) dispatch({ type: 'obs', streaming })
    }
    void onObsState((s) => apply(s.streaming)).then((fn) => {
      if (active) unlisten = fn
      else fn()
    })
    obsState().then(
      (s) => apply(s.streaming),
      () => {},
    )
    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  const select = useCallback((setup: Setup) => {
    dispatch({ type: 'tap', setup })
    try {
      localStorage.setItem(STORAGE_KEY, setup)
    } catch {
      // storage unavailable: the tap still applies for this session
    }
  }, [])

  return { selected: state.selected, select }
}
