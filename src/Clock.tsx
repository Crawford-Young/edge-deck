import { useEffect, useState } from 'react'

const MINUTE_MS = 60_000

/** Time over AM/PM: two short lines fit the rail at any hour. */
export function Clock() {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    const timer = setTimeout(() => setNow(new Date()), MINUTE_MS - (now.getTime() % MINUTE_MS))
    return () => clearTimeout(timer)
  }, [now])

  const hours = now.getHours()
  const minutes = String(now.getMinutes()).padStart(2, '0')

  return (
    <time
      dateTime={now.toISOString()}
      className="flex flex-col items-center font-semibold tabular-nums"
    >
      <span className="text-3xl">{`${hours % 12 || 12}:${minutes}`}</span>
      <span className="text-base text-muted-foreground">{hours < 12 ? 'AM' : 'PM'}</span>
    </time>
  )
}
