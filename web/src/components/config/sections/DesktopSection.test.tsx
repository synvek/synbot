import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { DesktopSection } from './DesktopSection'

const t = (key: string) => key
const LABEL = 'config.sections.desktop.closeToTray'

describe('DesktopSection', () => {
  it('shows an unchecked toggle when the config predates the desktop section', () => {
    render(<DesktopSection draft={{}} setDraft={vi.fn()} t={t} />)
    expect(screen.getByRole('switch', { name: LABEL })).toHaveAttribute('aria-checked', 'false')
  })

  it('turns the toggle into desktop.closeToTray and keeps the rest of the section', async () => {
    const setDraft = vi.fn()
    render(<DesktopSection draft={{ desktop: { keep: 1 } }} setDraft={setDraft} t={t} />)

    await userEvent.click(screen.getByRole('switch', { name: LABEL }))

    // `Config.tsx` owns the draft, so the section hands back an updater; apply it like it would.
    const updater = setDraft.mock.calls[0][0] as (d: Record<string, unknown>) => Record<string, unknown>
    expect(updater({ desktop: { keep: 1 } })).toEqual({ desktop: { keep: 1, closeToTray: true } })
  })
})
