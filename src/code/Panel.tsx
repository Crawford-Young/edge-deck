import { useId, type ReactNode } from 'react'

interface PanelProps {
  title: string
  count?: ReactNode
  children: ReactNode
}

/** A labelled panel frame: title row on top, body that scrolls inside itself. */
export function Panel({ title, count, children }: PanelProps) {
  const titleId = useId()
  return (
    <section
      aria-labelledby={titleId}
      className="relative flex min-h-0 flex-1 flex-col gap-4 rounded-xl border border-border bg-surface p-4"
    >
      <div className="flex h-8 shrink-0 items-center justify-between text-lg font-medium tracking-wider text-muted-foreground uppercase">
        <h2 id={titleId}>{title}</h2>
        {count}
      </div>
      <div className="relative min-h-0 flex-1 touch-pan-y overflow-y-auto">{children}</div>
    </section>
  )
}
