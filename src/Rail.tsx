import { ToggleGroup, ToggleGroupItem } from '@crawfordyoung/ui'
import { Radio, SquareTerminal } from 'lucide-react'
import { Clock } from './Clock'
import { isSetup, type Setup } from './setups'

export function Rail({
  selected,
  onSelect,
}: {
  selected: Setup
  onSelect: (setup: Setup) => void
}) {
  return (
    <nav
      aria-label="Setups"
      className="flex h-full w-28 shrink-0 flex-col justify-between border-r p-2"
    >
      <ToggleGroup
        type="single"
        orientation="vertical"
        size="touch"
        value={selected}
        onValueChange={(value) => {
          // Radix emits '' when the pressed item is tapped again: one setup stays selected.
          if (isSetup(value)) onSelect(value)
        }}
      >
        <ToggleGroupItem value="stream">
          <Radio aria-hidden />
          Stream
        </ToggleGroupItem>
        <ToggleGroupItem value="code">
          <SquareTerminal aria-hidden />
          Code
        </ToggleGroupItem>
      </ToggleGroup>
      <Clock />
    </nav>
  )
}
