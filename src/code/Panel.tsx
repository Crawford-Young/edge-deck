import { useId, type ReactNode } from 'react'

interface PanelProps {
  title: string
  count?: ReactNode
  children: ReactNode
  /** False for a body that fills the panel and never scrolls, such as a canvas. */
  scroll?: boolean
}

/** A labelled panel frame: title row on top, body that scrolls inside itself. */
export function Panel({ title, count, children, scroll = true }: PanelProps) {
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
      <div
        // A scroller must take focus so a keyboard can scroll it (axe scrollable-region-focusable).
        {...(scroll && { tabIndex: 0, role: 'group', 'aria-labelledby': titleId })}
        className={
          scroll
            ? 'relative min-h-0 flex-1 touch-pan-y overflow-y-auto rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
            : 'relative min-h-0 flex-1 overflow-hidden'
        }
      >
        {children}
      </div>
    </section>
  )
}
