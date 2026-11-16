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
    // Only the newest read may land; an older one resolving late is dropped.
    const read = () => {
      const mine = ++latest
      codeSnapshot().then(
        (snapshot) => {
          if (active && mine === latest) setState({ snapshot, error: null })
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
    void onCodeChanged(read).then((fn) => {
      if (active) unlisten = fn
      else fn()
    })
    read()
    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  return state
}
