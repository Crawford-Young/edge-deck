import { Rail } from './Rail'
import { useSetups } from './useSetups'

const NAMES = { stream: 'Stream', code: 'Code' } as const

export function App() {
  const { selected, select } = useSetups()
  return (
    <div className="flex h-full">
      <Rail selected={selected} onSelect={select} />
      <main className="relative min-w-0 flex-1 overflow-hidden">
        <h1 className="p-8 text-4xl font-semibold">{NAMES[selected]}</h1>
      </main>
    </div>
  )
}
