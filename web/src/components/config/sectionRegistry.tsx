import React from 'react'
import type { SectionProps } from './types'
import { GeneralSection } from './sections/GeneralSection'
import { ChannelsSection } from './sections/ChannelsSection'
import { ProvidersSection } from './sections/ProvidersSection'
import { MainAgentSection } from './sections/MainAgentSection'
import { MemorySection } from './sections/MemorySection'
import { ToolsSection } from './sections/ToolsSection'
import { WebSection } from './sections/WebSection'
import { LogSection } from './sections/LogSection'
import { HeartbeatSection } from './sections/HeartbeatSection'
import { CronSection } from './sections/CronSection'
import { WorkflowSection } from './sections/WorkflowSection'
import { SandboxSection } from './sections/SandboxSection'
import { PairingsSection } from './sections/PairingsSection'
import { PluginsSection } from './sections/PluginsSection'

export type ConfigSectionId =
  | 'general'
  | 'channels'
  | 'providers'
  | 'mainAgent'
  | 'memory'
  | 'tools'
  | 'web'
  | 'log'
  | 'heartbeat'
  | 'cron'
  | 'workflow'
  | 'sandbox'
  | 'pairings'
  | 'plugins'

export const SECTION_ORDER: ConfigSectionId[] = [
  'general',
  'channels',
  'providers',
  'mainAgent',
  'memory',
  'tools',
  'web',
  'log',
  'heartbeat',
  'cron',
  'workflow',
  'sandbox',
  'pairings',
  'plugins',
]

const REGISTRY: Record<ConfigSectionId, React.FC<SectionProps>> = {
  general: GeneralSection,
  channels: ChannelsSection,
  providers: ProvidersSection,
  mainAgent: MainAgentSection,
  memory: MemorySection,
  tools: ToolsSection,
  web: WebSection,
  log: LogSection,
  heartbeat: HeartbeatSection,
  cron: CronSection,
  workflow: WorkflowSection,
  sandbox: SandboxSection,
  pairings: PairingsSection,
  plugins: PluginsSection,
}

export function ConfigSectionBody(props: SectionProps & { id: ConfigSectionId }): React.ReactElement {
  const Cmp = REGISTRY[props.id]
  return <Cmp draft={props.draft} setDraft={props.setDraft} t={props.t} />
}
