const pad = (n: number) => String(n).padStart(2, '0')

const seconds = (ms: number) => Math.max(0, Math.floor(ms / 1000))

/** Elapsed time as h:mm:ss, e.g. "1:02:03". */
export function runningTime(ms: number): string {
  const total = seconds(ms)
  return `${Math.floor(total / 3600)}:${pad(Math.floor((total % 3600) / 60))}:${pad(total % 60)}`
}

/** A wait's age at its two largest units: "45s", "3m 05s", "2h 07m". */
export function age(ms: number): string {
  const total = seconds(ms)
  if (total < 60) return `${total}s`
  if (total < 3600) return `${Math.floor(total / 60)}m ${pad(total % 60)}s`
  return `${Math.floor(total / 3600)}h ${pad(Math.floor((total % 3600) / 60))}m`
}
