import { CodeSetup } from './code/CodeSetup'
import { Rail } from './Rail'
import { useCodeSnapshot } from './code/useCodeSnapshot'
import { useSetups } from './useSetups'

export function App() {
  const { selected, select } = useSetups()
  const { snapshot, error } = useCodeSnapshot()
  return (
    <div className="flex h-full">
      <Rail selected={selected} onSelect={select} />
      <main className="relative min-w-0 flex-1 overflow-hidden">
        {selected === 'code' ? (
          <CodeSetup snapshot={snapshot} error={error} />
        ) : (
          <h1 className="p-8 text-4xl font-semibold">Stream</h1>
        )}
      </main>
    </div>
  )
}
