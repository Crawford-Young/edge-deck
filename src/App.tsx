import { CodeSetup } from './code/CodeSetup'
import { Rail } from './Rail'
import { useSetups } from './useSetups'

export function App() {
  const { selected, select } = useSetups()
  return (
    <div className="flex h-full">
      <Rail selected={selected} onSelect={select} />
      <main className="relative min-w-0 flex-1 overflow-hidden">
        {selected === 'code' ? (
          <CodeSetup header={null} waits={[]} agentCount={0} />
        ) : (
          <h1 className="p-8 text-4xl font-semibold">Stream</h1>
        )}
      </main>
    </div>
  )
}
