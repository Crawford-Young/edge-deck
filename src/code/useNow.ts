import { useEffect, useState } from 'react'

const SECOND_MS = 1000

/** Epoch ms, refreshed every second; the one clock behind every running time and age. */
export function useNow(): number {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), SECOND_MS)
    return () => clearInterval(timer)
  }, [])

  return now
}
