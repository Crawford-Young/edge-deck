import { useEffect, useState } from 'react'
import { githubState, onGithubState, type GithubState } from '../api'

const LOADING: GithubState = { status: 'loading', prs: [], issues: [] }

/** The backend's GitHub state, kept current by `github-state` events. */
export function useGithub(): GithubState {
  const [state, setState] = useState<GithubState>(LOADING)

  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined
    const apply = (s: GithubState) => {
      if (active) setState(s)
    }
    // Subscribe first, then read; a read that lands after an event is stale and dropped.
    let seen = false
    void onGithubState((s) => {
      seen = true
      apply(s)
    }).then(
      (fn) => {
        if (!active) return fn()
        unlisten = fn
        githubState().then(
          (s) => {
            if (!seen) apply(s)
          },
          () => {},
        )
      },
      () => {
        if (!active) return
        // No live events: still read once, and say so when that fails too.
        githubState().then(apply, () => apply({ status: 'error', prs: [], issues: [] }))
      },
    )
    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  return state
}
