import { badgeVariants } from '@crawfordyoung/ui'
import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Checks, GithubIssue, GithubState, GithubStatus, PullRequest } from '../api'
import { GitHubPanel } from './GitHubPanel'
import { useGithub } from './useGithub'

vi.mock('./useGithub', () => ({ useGithub: vi.fn() }))

const pr = (
  number: number,
  title: string,
  checks: Checks = 'none',
  extra: Partial<PullRequest> = {},
): PullRequest => ({
  repo: 'octo/widgets',
  number,
  title,
  url: `https://example.test/octo/widgets/pull/${number}`,
  draft: false,
  checks,
  ...extra,
})

const issue = (number: number, title: string, repo = 'octo/gadgets'): GithubIssue => ({
  repo,
  number,
  title,
  url: `https://example.test/${repo}/issues/${number}`,
})

const show = (state: Partial<GithubState> & { status: GithubStatus }) => {
  vi.mocked(useGithub).mockReturnValue({ prs: [], issues: [], ...state })
  render(<GitHubPanel />)
  return screen.getByRole('region', { name: 'GitHub' })
}

/** The row holding `title`: its highest ancestor that holds no other listed title. */
function rowOf(region: HTMLElement, title: string, others: string[]): HTMLElement {
  let el = within(region).getByText(title)
  while (el.parentElement && el.parentElement !== region) {
    const text = el.parentElement.textContent ?? ''
    if (others.some((o) => o !== title && text.includes(o))) break
    el = el.parentElement
  }
  return el
}

// The library's Badge exposes tone only through its variant classes, so tone is
// read off `badgeVariants`: the tokens that name the tone, not hardcoded strings.
const toneTokens = (variant: 'destructive' | 'warning') =>
  badgeVariants({ variant })
    .split(' ')
    .filter((t) => t.includes(variant) && !t.startsWith('hover:'))

function classesAround(label: HTMLElement, row: HTMLElement): string {
  const classes: string[] = []
  for (let el: HTMLElement | null = label; el && el !== row.parentElement; el = el.parentElement) {
    classes.push(el.className)
    if (el === row) break
  }
  return classes.join(' ')
}

beforeEach(() => {
  vi.resetAllMocks()
})

describe('GitHubPanel', () => {
  it('is the GitHub region', () => {
    const region = show({ status: 'ok' })
    expect(within(region).getByRole('heading', { name: 'GitHub' })).toBeInTheDocument()
  })

  it.each<[GithubStatus, string]>([
    ['loading', 'Loading GitHub…'],
    ['gh-missing', 'GitHub CLI not found. Install gh to see PRs and issues.'],
    ['signed-out', 'Not signed in to GitHub. Run gh auth login.'],
    ['error', "Couldn't reach GitHub. Retrying."],
    ['ok', 'No open PRs or in-progress issues'],
  ])('shows the stated empty state for %s', (status, text) => {
    const region = show({ status })
    expect(within(region).getByText(text)).toBeInTheDocument()
  })

  it('shows only the empty state matching the status', () => {
    const region = show({ status: 'signed-out' })
    expect(within(region).queryByText('Loading GitHub…')).toBeNull()
    expect(within(region).queryByText('No open PRs or in-progress issues')).toBeNull()
  })

  it('hides the empty state once there is something to list', () => {
    const region = show({ status: 'ok', prs: [pr(1, 'Add a gizmo')] })
    expect(within(region).queryByText('No open PRs or in-progress issues')).toBeNull()
  })

  it('counts PRs and issues in the title row', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'One'), pr(2, 'Two')],
      issues: [issue(3, 'Three'), issue(4, 'Four'), issue(5, 'Five')],
    })
    expect(within(region).getByText('2 PRs · 3 issues')).toBeInTheDocument()
  })

  it('uses the singular for exactly one PR and one issue', () => {
    const region = show({ status: 'ok', prs: [pr(1, 'One')], issues: [issue(2, 'Two')] })
    expect(within(region).getByText('1 PR · 1 issue')).toBeInTheDocument()
  })

  it('lists PRs first, then issues, each in the order given', () => {
    const region = show({
      status: 'ok',
      prs: [pr(10, 'First PR'), pr(11, 'Second PR')],
      issues: [issue(20, 'First issue'), issue(21, 'Second issue')],
    })
    const titles = ['First PR', 'Second PR', 'First issue', 'Second issue']
    const nodes = titles.map((t) => within(region).getByText(t))
    const sorted = [...nodes].sort((x, y) =>
      x.compareDocumentPosition(y) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1,
    )
    expect(sorted).toEqual(nodes)
  })

  it('shows repo#number and title on a PR row and on an issue row', () => {
    const region = show({
      status: 'ok',
      prs: [pr(42, 'Add a gizmo')],
      issues: [issue(9, 'Gizmo is slow', 'octo/gadgets')],
    })
    const titles = ['Add a gizmo', 'Gizmo is slow']
    expect(rowOf(region, 'Add a gizmo', titles).textContent).toContain('octo/widgets#42')
    expect(rowOf(region, 'Gizmo is slow', titles).textContent).toContain('octo/gadgets#9')
  })

  it('labels a failing PR "Checks failing" in the destructive tone', () => {
    const region = show({ status: 'ok', prs: [pr(1, 'Red PR', 'failing')] })
    const label = within(region).getByText('Checks failing')
    const row = rowOf(region, 'Red PR', [])
    const classes = classesAround(label, row)
    for (const token of toneTokens('destructive')) expect(classes).toContain(token)
  })

  it('labels a passing PR "Checks passing" in a tone of its own, not destructive or warning', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'Green PR', 'passing'), pr(2, 'Red PR', 'failing')],
    })
    const titles = ['Green PR', 'Red PR']
    const green = rowOf(region, 'Green PR', titles)
    const label = within(green).getByText('Checks passing')
    const classes = classesAround(label, green)
    for (const variant of ['destructive', 'warning'] as const) {
      for (const token of toneTokens(variant)) expect(classes).not.toContain(token)
    }
    expect(classes.split(/\s+/)).toContain('bg-success')
    const red = rowOf(region, 'Red PR', titles)
    expect(classes).not.toBe(classesAround(within(red).getByText('Checks failing'), red))
  })

  it('labels a pending PR "Checks pending" without the destructive tone and unlike passing', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'Wait PR', 'pending'), pr(2, 'Green PR', 'passing')],
    })
    const titles = ['Wait PR', 'Green PR']
    const wait = rowOf(region, 'Wait PR', titles)
    const waitClasses = classesAround(within(wait).getByText('Checks pending'), wait)
    for (const token of toneTokens('destructive')) expect(waitClasses).not.toContain(token)
    for (const token of toneTokens('warning')) expect(waitClasses).not.toContain(token)
    expect(waitClasses.split(/\s+/)).not.toContain('bg-success')
    const green = rowOf(region, 'Green PR', titles)
    expect(waitClasses).not.toBe(classesAround(within(green).getByText('Checks passing'), green))
  })

  it('shows no check label for a PR without checks', () => {
    const region = show({ status: 'ok', prs: [pr(1, 'Bare PR', 'none')] })
    expect(within(region).queryByText(/^Checks /)).toBeNull()
  })

  it('shows "Draft" on a draft PR only', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'WIP thing', 'none', { draft: true }), pr(2, 'Ready thing')],
    })
    const titles = ['WIP thing', 'Ready thing']
    expect(within(rowOf(region, 'WIP thing', titles)).getByText('Draft')).toBeInTheDocument()
    expect(within(rowOf(region, 'Ready thing', titles)).queryByText('Draft')).toBeNull()
  })

  it('shows both Draft and the check label on a draft PR with failing checks', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'WIP thing', 'failing', { draft: true })],
    })
    expect(within(region).getByText('Draft')).toBeInTheDocument()
    expect(within(region).getByText('Checks failing')).toBeInTheDocument()
  })

  it('does not label issue rows with checks or draft', () => {
    const region = show({ status: 'ok', issues: [issue(1, 'Only issue')] })
    expect(within(region).queryByText(/^Checks /)).toBeNull()
    expect(within(region).queryByText('Draft')).toBeNull()
  })

  it('has no tappable rows in pass 1', () => {
    const region = show({
      status: 'ok',
      prs: [pr(1, 'A PR', 'passing')],
      issues: [issue(2, 'An issue')],
    })
    expect(within(region).queryAllByRole('button')).toHaveLength(0)
    expect(within(region).queryAllByRole('link')).toHaveLength(0)
  })
})
