import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { CodeSnapshot, FocusTarget, SessionKind } from './code/types'

export type { CodeSnapshot, FocusTarget, SessionKind }

/** "off" covers OBS closed, its WebSocket server disabled, or no OBS settings file. */
export type ObsConnection = 'off' | 'connecting' | 'connected' | 'auth-failed'

export interface ObsState {
  connection: ObsConnection
  streaming: boolean
}

/** The backend's current OBS state. */
export function obsState(): Promise<ObsState> {
  return invoke<ObsState>('obs_state')
}

/**
 * Bring the session's terminal tab or window to the front, or open a terminal
 * attached to a background session. Rejects with the backend's message.
 */
export function focusTerminal(session: FocusTarget): Promise<void> {
  const { pid, sessionId, title, kind } = session
  return invoke<undefined>('focus_terminal', { pid, sessionId, title, kind })
}

/** Call `cb` on every OBS state change; resolves to the unsubscribe function. */
export function onObsState(cb: (state: ObsState) => void): Promise<UnlistenFn> {
  return listen<ObsState>('obs-state', (event) => cb(event.payload))
}

/** "loading" until the first `gh` run returns; every state but "ok" has empty lists. */
export type GithubStatus = 'loading' | 'ok' | 'gh-missing' | 'signed-out' | 'error'

/** A PR's latest-commit check rollup; "none" when it has no checks. */
export type Checks = 'passing' | 'failing' | 'pending' | 'none'

export interface PullRequest {
  /** owner/name */
  repo: string
  number: number
  title: string
  url: string
  draft: boolean
  checks: Checks
}

export interface GithubIssue {
  /** owner/name */
  repo: string
  number: number
  title: string
  url: string
}

export interface GithubState {
  status: GithubStatus
  prs: PullRequest[]
  issues: GithubIssue[]
}

/** The backend's current GitHub state. */
export function githubState(): Promise<GithubState> {
  return invoke<GithubState>('github_state')
}

/** Call `cb` on every GitHub state change; resolves to the unsubscribe function. */
export function onGithubState(cb: (state: GithubState) => void): Promise<UnlistenFn> {
  return listen<GithubState>('github-state', (event) => cb(event.payload))
}

/** The backend's current Code snapshot; rejects with the backend's message. */
export function codeSnapshot(): Promise<CodeSnapshot> {
  return invoke<CodeSnapshot>('code_snapshot')
}

/** Call `cb` on every Code change (a payload-less event); resolves to the unsubscribe function. */
export function onCodeChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<null>('code-changed', () => cb())
}
