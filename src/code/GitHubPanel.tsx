import { Badge, type BadgeProps } from '@crawfordyoung/ui'
import type { ReactNode } from 'react'
import type { Checks, GithubIssue, GithubStatus, PullRequest } from '../api'
import { Panel } from './Panel'
import { useGithub } from './useGithub'

const EMPTY: Record<GithubStatus, string> = {
  loading: 'Loading GitHub…',
  'gh-missing': 'GitHub CLI not found. Install gh to see PRs and issues.',
  'signed-out': 'Not signed in to GitHub. Run gh auth login.',
  error: "Couldn't reach GitHub. Retrying.",
  ok: 'No open PRs or in-progress issues',
}

const BADGE = 'h-9 px-4 text-xl'

type CheckStyle = { label: string; variant: BadgeProps['variant']; className?: string }

const CHECKS: Record<Exclude<Checks, 'none'>, CheckStyle> = {
  failing: { label: 'Checks failing', variant: 'destructive' },
  // Badge has no success variant; these are the library's success tokens.
  passing: {
    label: 'Checks passing',
    variant: 'outline',
    className: 'border-transparent bg-success text-success-foreground',
  },
  pending: { label: 'Checks pending', variant: 'secondary' },
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

function CheckBadge({ checks }: { checks: Checks }) {
  if (checks === 'none') return null
  const { label, variant, className = '' } = CHECKS[checks]
  return (
    <Badge variant={variant} className={`${BADGE} ${className}`}>
      {label}
    </Badge>
  )
}

function Row({ item, children }: { item: PullRequest | GithubIssue; children?: ReactNode }) {
  return (
    <li className="flex min-h-24 flex-col justify-center gap-2 rounded-lg border border-border bg-background p-4">
      <div className="flex items-center justify-between gap-3">
        <span className="font-mono text-[22px] text-muted-foreground">{`${item.repo}#${item.number}`}</span>
        <div className="flex shrink-0 gap-2">{children}</div>
      </div>
      <span className="text-[26px] font-semibold">{item.title}</span>
    </li>
  )
}

/** Open PRs with their checks, then in-progress issues, from the GitHub refresher. */
export function GitHubPanel() {
  const { status, prs, issues } = useGithub()
  const listed = status === 'ok' && prs.length + issues.length > 0
  const count = status === 'ok' && (
    <span className="normal-case">
      {`${plural(prs.length, 'PR', 'PRs')} · ${plural(issues.length, 'issue', 'issues')}`}
    </span>
  )
  return (
    <Panel title="GitHub" count={count}>
      {listed ? (
        <ul className="flex flex-col gap-3">
          {prs.map((pr) => (
            <Row key={`pr ${pr.repo}#${pr.number}`} item={pr}>
              {pr.draft && (
                <Badge variant="outline" className={BADGE}>
                  Draft
                </Badge>
              )}
              <CheckBadge checks={pr.checks} />
            </Row>
          ))}
          {issues.map((issue) => (
            <Row key={`issue ${issue.repo}#${issue.number}`} item={issue} />
          ))}
        </ul>
      ) : (
        <p className="text-xl text-muted-foreground">{EMPTY[status]}</p>
      )}
    </Panel>
  )
}
