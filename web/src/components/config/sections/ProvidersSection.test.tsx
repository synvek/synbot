import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { ProvidersSection } from './ProvidersSection';
import { MainAgentSection } from './MainAgentSection';
import type { ProviderCatalog, ProviderMetadata } from '../../../types/api';

const { catalog } = vi.hoisted(() => {
  const metadata = (
    overrides: Partial<ProviderMetadata> & Pick<ProviderMetadata, 'key' | 'label'>
  ): ProviderMetadata => ({
    aliases: [overrides.key],
    apiStyle: 'openai',
    requiresKey: true,
    local: false,
    embeddings: true,
    defaultBase: 'https://example.test/v1',
    docsUrl: 'https://example.test/models',
    apiBase: null,
    apiBaseEffective: 'https://example.test/v1',
    maxTokensCap: null,
    configured: false,
    usedBy: [],
    ...overrides,
  });
  return {
    catalog: {
      providers: [
        metadata({
          key: 'anthropic',
          label: 'Anthropic',
          apiStyle: 'anthropic',
          embeddings: false,
          defaultBase: 'https://api.anthropic.com',
          apiBaseEffective: 'https://gateway.example.com',
          apiBase: 'https://gateway.example.com/',
          configured: true,
          usedBy: ['main'],
        }),
        metadata({ key: 'ollama', label: 'Ollama', requiresKey: false, local: true, configured: true }),
      ],
      extra: [
        {
          name: 'minimax',
          apiStyle: 'anthropic',
          apiBase: 'https://api.minimax.test',
          apiBaseEffective: 'https://api.minimax.test',
          maxTokensCap: null,
          configured: true,
          usedBy: ['main'],
        },
      ],
      unknownInUse: [],
      defaults: { provider: 'minimax', model: 'MiniMax-M3' },
    } satisfies ProviderCatalog,
  };
});

vi.mock('../useProviderCatalog', () => ({
  useProviderCatalog: () => catalog,
}));

// The sections only need a translator; the real one lives in I18nProvider and would add noise.
const dict: Record<string, string> = {
  'config.sections.providers.apiKey': 'API key',
  'config.sections.providers.apiBase': 'API base URL',
  'config.sections.providers.apiStyle': 'API style (extra providers)',
  'config.sections.providers.maxTokensCap': 'Max tokens cap',
  'config.sections.providers.extra': 'Extra providers',
  'config.sections.providers.extraHint': 'Arbitrary names.',
  'config.sections.providers.extraPrompt': 'New provider key',
  'config.sections.providers.addExtra': 'Add extra provider',
  'config.sections.providers.builtin.anthropic': 'Anthropic',
  'config.sections.providers.builtin.ollama': 'Ollama',
  'config.sections.providers.configured': 'Key set',
  'config.sections.providers.notConfigured': 'No key',
  'config.sections.providers.local': 'Local runtime',
  'config.sections.providers.usedBy': 'Selected by',
  'config.sections.providers.mainRole': 'main agent',
  'config.sections.providers.baseDefault': 'Default endpoint',
  'config.sections.providers.baseOverride': 'Custom endpoint',
  'config.sections.providers.embeddingsYes': 'Embeddings: supported',
  'config.sections.providers.embeddingsNo': 'Embeddings: not supported',
  'config.sections.providers.modelsLink': 'Model list',
  'config.sections.providers.styleFixed': 'Wire format (fixed by name):',
  'config.secretLeaveUnchanged': 'Leave unchanged',
  'config.sections.mainAgent.workspace': 'Workspace',
  'config.sections.mainAgent.provider': 'Default provider',
  'config.sections.mainAgent.model': 'Default model',
  'config.sections.mainAgent.providerUnset': '(not set)',
  'config.sections.mainAgent.modelHint': 'Model ids come from the provider.',
  'config.sections.mainAgent.agents': 'Additional agents',
  'config.sections.mainAgent.agentsHint': 'Extra agents.',
  'config.sections.mainAgent.addAgent': 'Add agent',
  'config.sections.mainAgent.agentProvider': 'Provider override',
  'config.sections.mainAgent.agentModel': 'Model override',
  'common.delete': 'Delete',
};
const t = (key: string) => dict[key] ?? key;

describe('ProvidersSection provider metadata', () => {
  it('shows configured state, effective endpoint and the fixed wire format', () => {
    render(
      <ProvidersSection
        draft={{ providers: { anthropic: { apiKey: '••••', apiBase: 'https://gateway.example.com/' } } }}
        setDraft={() => {}}
        t={t}
      />
    );

    // Status comes from /api/providers, not from whether the key field looks empty.
    // Both mocked providers are configured, so the badge is not unique to the Anthropic card.
    expect(screen.getAllByText('Key set').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Selected by: main agent').length).toBeGreaterThan(0);
    expect(screen.getByText('Custom endpoint')).toBeTruthy();
    expect(screen.getByText('https://gateway.example.com')).toBeTruthy();
    expect(screen.getByText('Embeddings: not supported')).toBeTruthy();

    // `apiStyle` is ignored for built-ins, so the Anthropic card shows it read-only. (Providers
    // without catalog metadata still fall back to the editable select; that path is the
    // "older daemon" behaviour and is intentionally untouched.)
    const card = within(
      screen.getByText('https://gateway.example.com').closest('div.border') as HTMLElement,
    );
    expect(card.queryByLabelText('API style (extra providers)')).toBeNull();
    expect(card.getByText('anthropic')).toBeTruthy();
  });

  it('marks local runtimes as configured without a key', () => {
    render(<ProvidersSection draft={{ providers: {} }} setDraft={() => {}} t={t} />);

    expect(screen.getAllByText('Key set')).toHaveLength(2);
    expect(screen.queryAllByText('No key')).toHaveLength(0);
    expect(screen.getByText('Local runtime')).toBeTruthy();
  });

  it('reports extra providers with their configured state', () => {
    render(
      <ProvidersSection
        draft={{ providers: { extra: { minimax: { apiKey: '••••' } } } }}
        setDraft={() => {}}
        t={t}
      />
    );

    expect(screen.getByText('minimax')).toBeTruthy();
    // Anthropic and the extra entry are both selected by the main agent.
    expect(screen.getAllByText('Selected by: main agent')).toHaveLength(2);
  });
});

describe('MainAgentSection provider picker', () => {
  const draft = {
    mainAgent: {
      provider: 'minimax',
      model: 'MiniMax-M3',
      agents: [{ name: 'dev', role: 'dev', provider: 'ghost-provider', model: 'ghost-model' }],
    },
  };

  it('offers built-in and extra providers, keeping an unknown current value', () => {
    render(<MainAgentSection draft={draft} setDraft={() => {}} t={t} />);

    const select = screen.getByLabelText('Default provider') as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toContain('anthropic');
    expect(values).toContain('minimax');
    expect(select.value).toBe('minimax');

    // An agent pointing at a name that is neither built-in nor extra must not be rewritten.
    const agentSelect = screen.getByLabelText('Provider override') as HTMLSelectElement;
    expect(Array.from(agentSelect.options).map((o) => o.value)).toContain('ghost-provider');
    expect(agentSelect.value).toBe('ghost-provider');
  });

  it('suggests model ids already used in this config', () => {
    render(<MainAgentSection draft={draft} setDraft={() => {}} t={t} />);

    const datalist = document.getElementById('synbot-model-ids');
    expect(datalist).toBeTruthy();
    const values = Array.from(datalist!.querySelectorAll('option')).map((o) => o.getAttribute('value'));
    expect(values).toContain('MiniMax-M3');
    expect(values).toContain('ghost-model');
    // Free text stays possible — the input is not restricted to the list.
    expect(screen.getByLabelText('Default model').getAttribute('list')).toBe('synbot-model-ids');
  });
});
