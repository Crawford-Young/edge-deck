import { useEffect, useState } from 'react'
import { codeSnapshot, onCodeChanged, type CodeSnapshot } from '../api'

export interface CodeSnapshotState {
  snapshot: CodeSnapshot | null
  error: string | null
}

/** The backend's Code snapshot, re-read on every `code-changed` event; no polling. */
export function useCodeSnapshot(): CodeSnapshotState {
  const [state, setState] = useState<CodeSnapshotState>({ snapshot: null, error: null })

  useEffect(() => {
    let active = true
    let latest = 0
    let unlisten: (() => void) | undefined
    // Set when the listener could not be registered: reads still work, live updates do not.
    let listenError: string | null = null
    // Only the newest read may land; an older one resolving late is dropped.
    const read = () => {
      const mine = ++latest
      codeSnapshot().then(
        (snapshot) => {
          if (active && mine === latest) setState({ snapshot, error: listenError })
        },
        (e: unknown) => {
          if (active && mine === latest)
            setState((prev) => ({
              snapshot: prev.snapshot,
              error: e instanceof Error ? e.message : String(e),
            }))
        },
      )
    }
    // Subscribe first, then read, so a change in between is not missed.
    void onCodeChanged(read).then(
      (fn) => {
        if (!active) return fn()
        unlisten = fn
        read()
      },
      (e: unknown) => {
        if (!active) return
        listenError = e instanceof Error ? e.message : String(e)
        read()
      },
    )
    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  return state
}
