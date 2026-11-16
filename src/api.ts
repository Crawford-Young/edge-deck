import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

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

/** Call `cb` on every OBS state change; resolves to the unsubscribe function. */
export function onObsState(cb: (state: ObsState) => void): Promise<UnlistenFn> {
  return listen<ObsState>('obs-state', (event) => cb(event.payload))
}
