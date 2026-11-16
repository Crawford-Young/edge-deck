import type { ReactNode } from 'react'
import { AgentsPanel } from './AgentsPanel'
import { GitHubPanel } from './GitHubPanel'
import { Header } from './Header'
import { NeedsYouPanel } from './NeedsYouPanel'
import type { HeaderData, Wait } from './types'

export interface CodeSetupProps {
  header: HeaderData | null
  waits: readonly Wait[]
  agentCount: number
  agents?: ReactNode
}

export function CodeSetup({ header, waits, agentCount, agents }: CodeSetupProps) {
  return (
    <div className="relative flex h-full flex-col gap-4 p-4">
      <h1 className="sr-only">Code</h1>
      <Header data={header} />
      <div className="flex min-h-0 flex-1 gap-4">
        <div className="flex w-[580px] shrink-0 flex-col">
          <NeedsYouPanel waits={waits} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <AgentsPanel agentCount={agentCount}>{agents}</AgentsPanel>
        </div>
        <div className="flex w-[640px] shrink-0 flex-col">
          <GitHubPanel />
        </div>
      </div>
    </div>
  )
}
