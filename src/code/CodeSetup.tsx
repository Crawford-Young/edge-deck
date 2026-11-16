import type { ReactNode } from 'react'
import type { CodeSnapshot } from '../api'
import { AgentsPanel } from './AgentsPanel'
import { GitHubPanel } from './GitHubPanel'
import { Header } from './Header'
import { NeedsYouPanel } from './NeedsYouPanel'

export interface CodeSetupProps {
  snapshot: CodeSnapshot | null
  /** A failed snapshot read's message. */
  error?: string | null
  agents?: ReactNode
}

export function CodeSetup({ snapshot, error, agents }: CodeSetupProps) {
  return (
    <div className="relative flex h-full flex-col gap-4 p-4">
      <h1 className="sr-only">Code</h1>
      <Header data={snapshot?.header ?? null} />
      {error && (
        <p role="alert" className="text-xl text-destructive">
          {error}
        </p>
      )}
      <div className="flex min-h-0 flex-1 gap-4">
        <div className="flex w-[580px] shrink-0 flex-col">
          <NeedsYouPanel waits={snapshot?.needsYou ?? []} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <AgentsPanel agentCount={snapshot?.header.liveAgents ?? 0}>{agents}</AgentsPanel>
        </div>
        <div className="flex w-[640px] shrink-0 flex-col">
          <GitHubPanel />
        </div>
      </div>
    </div>
  )
}
