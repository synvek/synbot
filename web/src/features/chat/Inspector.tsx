import type { ReactNode } from 'react'

interface InspectorProps {
  children: ReactNode
  /** Accessible name: the panel holds the workspace tree, diffs and deliverables. */
  label: string
}

/**
 * The right-hand workspace panel: fixed 320px, separated by a single rule, never a floating card.
 * It disappears below `lg` and in focus mode (⌘.), where the transcript takes the whole width.
 *
 * A `<section>` rather than `<aside>`: it lives inside `<main>`, and a complementary landmark
 * nested in another landmark is an accessibility violation.
 */
export function Inspector({ children, label }: InspectorProps) {
  return (
    <section
      aria-label={label}
      className="hidden min-h-0 w-80 shrink-0 flex-col border-l border-line bg-surface lg:flex"
    >
      {children}
    </section>
  )
}
