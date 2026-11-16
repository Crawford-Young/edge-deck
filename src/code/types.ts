export interface HeaderData {
  startedAt: number
  sessionUsd: number
  todayUsd: number
  liveAgents: number
  needYou: number
}

export interface Wait {
  id: string
  label: string
  waitsFor: string
  since: number
}
