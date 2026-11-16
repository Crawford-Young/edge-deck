import { useEffect, useState } from 'react'

const MINUTE_MS = 60_000

export function Clock() {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    const timer = setTimeout(() => setNow(new Date()), MINUTE_MS - (now.getTime() % MINUTE_MS))
    return () => clearTimeout(timer)
  }, [now])

  return (
    <time dateTime={now.toISOString()} className="text-center text-2xl font-semibold tabular-nums">
      {now.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
    </time>
  )
}
