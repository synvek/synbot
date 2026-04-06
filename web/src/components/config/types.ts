import type React from 'react'

export type ConfigUpdater = React.Dispatch<React.SetStateAction<Record<string, unknown>>>

export type SectionProps = {
  draft: Record<string, unknown>
  setDraft: ConfigUpdater
  t: (key: string) => string
}
