import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { PlanCard } from './PlanCard'
import { I18nProvider } from '../../i18n/I18nContext'
import type { PendingPlan } from '../../types/agentEvents'

const plan: PendingPlan = {
  id: 'plan-1a2b3c4d',
  summary: '先把会话维度打通，再补前端。',
  steps: [
    { id: 's1', title: '扩展 SessionMeta', status: 'pending' },
    { id: 's2', title: '补 WS 协议', status: 'pending' },
  ],
  created_at: '2026-11-03T10:30:00Z',
}

const renderWithI18n = (component: React.ReactElement) =>
  render(<I18nProvider>{component}</I18nProvider>)

describe('PlanCard', () => {
  it('renders the summary and every planned step', () => {
    renderWithI18n(<PlanCard plan={plan} approved={false} onDecide={vi.fn()} />)

    expect(screen.getByText(plan.summary)).toBeInTheDocument()
    for (const step of plan.steps) {
      expect(screen.getByText(step.title)).toBeInTheDocument()
    }
  })

  it('approves once and reports the plan as running until the turn starts', () => {
    const onDecide = vi.fn()
    renderWithI18n(<PlanCard plan={plan} approved={false} onDecide={onDecide} />)

    const approve = screen.getByRole('button', { name: /批准执行|Approve and run/ })
    fireEvent.click(approve)
    fireEvent.click(approve)

    expect(onDecide).toHaveBeenCalledTimes(1)
    expect(onDecide).toHaveBeenCalledWith(true)
    // The card outlives the click: it says the plan is executing, not that it is gone.
    expect(screen.queryByRole('button', { name: /批准执行|Approve and run/ })).not.toBeInTheDocument()
    expect(screen.getByText(/执行中|Running/)).toBeInTheDocument()
  })

  it('cancels without asking for execution', () => {
    const onDecide = vi.fn()
    renderWithI18n(<PlanCard plan={plan} approved={false} onDecide={onDecide} />)

    fireEvent.click(screen.getByRole('button', { name: /取消|Cancel/ }))

    expect(onDecide).toHaveBeenCalledWith(false)
    expect(screen.getByText(/已取消|Cancelled/)).toBeInTheDocument()
  })

  it('shows no decisions once the approved turn is starting', () => {
    renderWithI18n(<PlanCard plan={plan} approved onDecide={vi.fn()} />)

    expect(screen.queryByRole('button', { name: /批准执行|Approve and run/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /取消|Cancel/ })).not.toBeInTheDocument()
  })
})
