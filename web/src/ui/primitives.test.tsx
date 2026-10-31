import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import {
  Badge,
  Button,
  CodeBlock,
  DataTable,
  Dialog,
  EmptyState,
  Field,
  Input,
  Switch,
  Tabs,
  ToastProvider,
  useToast,
} from './index'
import type { Column } from './Table'

describe('Button', () => {
  it('runs its click handler and exposes a busy state while loading', () => {
    const onClick = vi.fn()
    const { rerender } = render(<Button onClick={onClick}>Save</Button>)
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(onClick).toHaveBeenCalledTimes(1)

    rerender(
      <Button onClick={onClick} loading>
        Save
      </Button>
    )
    const button = screen.getByRole('button', { name: 'Save' })
    expect(button).toBeDisabled()
    expect(button).toHaveAttribute('aria-busy', 'true')
  })
})

describe('Field', () => {
  it('labels its control and shows the error instead of the hint', () => {
    render(
      <Field label="Workspace" htmlFor="ws" hint="Absolute path" error="Required">
        <Input id="ws" />
      </Field>
    )
    expect(screen.getByLabelText('Workspace')).toBeInTheDocument()
    expect(screen.queryByText('Absolute path')).not.toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('Required')
  })
})

describe('Switch', () => {
  it('reports its state through aria-checked', async () => {
    const onChange = vi.fn()
    render(<Switch checked={false} onChange={onChange} label="Enable channel" />)
    const control = screen.getByRole('switch', { name: 'Enable channel' })
    expect(control).toHaveAttribute('aria-checked', 'false')
    await userEvent.click(control)
    expect(onChange).toHaveBeenCalledWith(true)
  })
})

describe('DataTable', () => {
  type Row = { id: string; name: string }

  const columns: Array<Column<Row>> = [
    { key: 'name', header: 'Name', render: (row) => row.name },
    { key: 'id', header: 'ID', render: (row) => row.id, align: 'right' },
  ]

  it('renders a header row and one row per item', () => {
    render(
      <DataTable
        rows={[{ id: 'a', name: 'Alpha' }]}
        columns={columns}
        rowKey={(row) => row.id}
      />
    )
    expect(screen.getByRole('columnheader', { name: 'Name' })).toBeInTheDocument()
    expect(screen.getByRole('cell', { name: 'Alpha' })).toBeInTheDocument()
  })

  it('renders the empty slot when there are no rows', () => {
    render(
      <DataTable rows={[]} columns={columns} rowKey={(row) => row.id} empty={<EmptyState title="Nothing yet" />} />
    )
    expect(screen.getByText('Nothing yet')).toBeInTheDocument()
  })
})

describe('Tabs', () => {
  it('moves the selection with the arrow keys', () => {
    const onChange = vi.fn()
    render(
      <Tabs
        items={[
          { id: 'chat', label: 'Chat' },
          { id: 'terminal', label: 'Terminal' },
        ]}
        value="chat"
        onChange={onChange}
      />
    )
    const chat = screen.getByRole('tab', { name: 'Chat' })
    fireEvent.keyDown(chat, { key: 'ArrowRight' })
    expect(onChange).toHaveBeenCalledWith('terminal')
  })
})

describe('Dialog', () => {
  it('opens modally and reports closing through its own state', () => {
    const onClose = vi.fn()
    const { rerender, container } = render(
      <Dialog open onClose={onClose} title="Remove channel">
        <p>Body copy</p>
      </Dialog>
    )
    const dialog = container.querySelector('dialog')
    expect(dialog).not.toBeNull()
    expect(dialog?.hasAttribute('open')).toBe(true)
    expect(screen.getByText('Body copy')).toBeInTheDocument()

    rerender(
      <Dialog open={false} onClose={onClose} title="Remove channel">
        <p>Body copy</p>
      </Dialog>
    )
    expect(container.querySelector('dialog')?.hasAttribute('open')).toBe(false)
  })
})

describe('Toast', () => {
  function Harness() {
    const { toast } = useToast()
    return <Button onClick={() => toast('Saved', 'success')}>Save</Button>
  }

  it('announces a pushed message in a live region', async () => {
    render(
      <ToastProvider>
        <Harness />
      </ToastProvider>
    )
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(await screen.findByText('Saved')).toBeInTheDocument()
  })
})

describe('CodeBlock', () => {
  it('renders the code with an accessible copy control', () => {
    render(<CodeBlock title="src/main.rs">{'fn main() {}'}</CodeBlock>)
    expect(screen.getByText('fn main() {}')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Copy' })).toBeInTheDocument()
  })
})

describe('Badge', () => {
  it('keeps the text as the accessible name', () => {
    render(<Badge tone="success">Connected</Badge>)
    expect(screen.getByText('Connected')).toBeInTheDocument()
  })
})
