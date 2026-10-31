import { useMemo, useState, type ReactNode } from 'react'
import { ChevronDown, ChevronUp, ChevronsUpDown } from 'lucide-react'
import { cn } from './cn'

export interface Column<T> {
  key: string
  header: ReactNode
  render: (row: T) => ReactNode
  align?: 'left' | 'right'
  /** Fixed width so numeric columns do not jump between renders. */
  width?: string
  className?: string
  /** Provide a comparable value to make the column sortable. */
  sortValue?: (row: T) => string | number
}

interface DataTableProps<T> {
  rows: T[]
  columns: Array<Column<T>>
  rowKey: (row: T) => string
  /** Rendered inside the table body when there are no rows. */
  empty?: ReactNode
  density?: 'comfortable' | 'compact'
  /** Optional row-level click; the row then behaves like a button for keyboard users. */
  onRowClick?: (row: T) => void
  selectedKey?: string | null
  className?: string
}

/**
 * The one table in the console. Sticky header, 1px row separators, no card shell, no zebra —
 * a table is a table. The wrapper owns horizontal scrolling so a narrow window never breaks the
 * page layout. Columns that expose `sortValue` sort on header click, in place: no server round trip.
 */
export function DataTable<T>({
  rows,
  columns,
  rowKey,
  empty,
  density = 'comfortable',
  onRowClick,
  selectedKey,
  className,
}: DataTableProps<T>) {
  const rowHeight = density === 'compact' ? 'h-9' : 'h-10'
  const [sort, setSort] = useState<{ key: string; direction: 'asc' | 'desc' } | null>(null)

  const sorted = useMemo(() => {
    if (!sort) return rows
    const column = columns.find((item) => item.key === sort.key)
    if (!column?.sortValue) return rows
    const factor = sort.direction === 'asc' ? 1 : -1
    return [...rows].sort((a, b) => {
      const left = column.sortValue!(a)
      const right = column.sortValue!(b)
      if (typeof left === 'number' && typeof right === 'number') return (left - right) * factor
      return String(left).localeCompare(String(right)) * factor
    })
  }, [columns, rows, sort])

  const toggleSort = (key: string) => {
    setSort((current) =>
      current?.key === key
        ? current.direction === 'asc'
          ? { key, direction: 'desc' }
          : null
        : { key, direction: 'asc' }
    )
  }

  return (
    <div className={cn('w-full overflow-x-auto', className)}>
      <table className="w-full min-w-[640px] border-collapse text-left">
        <thead>
          <tr className="sticky top-0 z-10 bg-surface-2">
            {columns.map((column) => {
              const active = sort?.key === column.key ? sort.direction : null
              return (
                <th
                  key={column.key}
                  scope="col"
                  aria-sort={active === 'asc' ? 'ascending' : active === 'desc' ? 'descending' : undefined}
                  style={column.width ? { width: column.width } : undefined}
                  className={cn(
                    'h-8 whitespace-nowrap border-b border-line px-3 text-micro font-medium uppercase tracking-wide text-ink-3',
                    column.align === 'right' ? 'text-right' : 'text-left'
                  )}
                >
                  {column.sortValue ? (
                    <button
                      type="button"
                      onClick={() => toggleSort(column.key)}
                      className={cn(
                        'inline-flex items-center gap-1 transition-colors duration-fast hover:text-ink',
                        column.align === 'right' && 'flex-row-reverse'
                      )}
                    >
                      {column.header}
                      {active === 'asc' ? (
                        <ChevronUp className="h-3 w-3" aria-hidden="true" />
                      ) : active === 'desc' ? (
                        <ChevronDown className="h-3 w-3" aria-hidden="true" />
                      ) : (
                        <ChevronsUpDown className="h-3 w-3 opacity-50" aria-hidden="true" />
                      )}
                    </button>
                  ) : (
                    column.header
                  )}
                </th>
              )
            })}
          </tr>
        </thead>
        <tbody>
          {sorted.length === 0 ? (
            <tr>
              <td colSpan={columns.length} className="px-3 py-10 text-center text-small text-ink-2">
                {empty ?? null}
              </td>
            </tr>
          ) : (
            sorted.map((row) => {
              const key = rowKey(row)
              const selected = selectedKey === key
              return (
                <tr
                  key={key}
                  tabIndex={onRowClick ? 0 : undefined}
                  onClick={onRowClick ? () => onRowClick(row) : undefined}
                  onKeyDown={
                    onRowClick
                      ? (event) => {
                          if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault()
                            onRowClick(row)
                          }
                        }
                      : undefined
                  }
                  aria-selected={selected || undefined}
                  className={cn(
                    rowHeight,
                    'border-b border-line align-middle text-small text-ink',
                    onRowClick && 'cursor-pointer',
                    selected ? 'bg-surface-2' : 'bg-surface',
                    onRowClick && !selected && 'hover-tint'
                  )}
                >
                  {columns.map((column) => (
                    <td
                      key={column.key}
                      className={cn(
                        'px-3',
                        column.align === 'right' ? 'text-right tabular' : 'text-left',
                        column.className
                      )}
                    >
                      {column.render(row)}
                    </td>
                  ))}
                </tr>
              )
            })
          )}
        </tbody>
      </table>
    </div>
  )
}
