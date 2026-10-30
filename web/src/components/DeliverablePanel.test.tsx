import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import DeliverablePanel from './DeliverablePanel';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api';
import type {
  Deliverable,
  DeliverableList,
  DeliverablePreviewResponse,
  Project,
} from '../types/api';

vi.mock('../api', () => ({
  apiClient: {
    getProjectDeliverables: vi.fn(),
    getDeliverablePreview: vi.fn(),
    downloadDeliverable: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const project: Project = {
  id: 'p1',
  name: 'demo',
  path: '/tmp/demo',
  added_at: '2026-01-01T00:00:00Z',
  git_head: null,
  rules: [],
};

const entry = (over: Partial<Deliverable>): Deliverable => ({
  name: 'report.docx',
  path: 'out/report.docx',
  kind: 'docx',
  size: 2048,
  modified_ms: 1_700_000_000_000,
  ...over,
});

const listing = (entries: Deliverable[], over: Partial<DeliverableList> = {}): DeliverableList => ({
  path: '',
  entries,
  truncated: false,
  scanned: entries.length,
  ...over,
});

const previewOf = (
  over: Partial<DeliverablePreviewResponse['preview']>,
  name = 'report.docx'
): DeliverablePreviewResponse => ({
  path: `out/${name}`,
  name,
  preview: { kind: 'docx', size: 2048, passthrough: false, truncated: false, ...over },
});

const renderPanel = () =>
  render(
    <I18nProvider>
      <DeliverablePanel project={project} />
    </I18nProvider>
  );

const createObjectURL = vi.fn(() => 'blob:mock');
const revokeObjectURL = vi.fn();

describe('DeliverablePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(URL, { createObjectURL, revokeObjectURL });
    // jsdom cannot navigate for a download anchor; the click itself is what we assert on.
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    mocked.getProjectDeliverables.mockResolvedValue(listing([]));
    mocked.downloadDeliverable.mockResolvedValue(new Blob(['%PDF-1.7']));
  });

  it('lists the generated documents with their kind and size', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(
      listing([
        entry({ name: 'report.docx', path: 'out/report.docx', size: 2048 }),
        entry({ name: 'budget.xlsx', path: 'out/budget.xlsx', kind: 'xlsx', size: 4096 }),
      ])
    );
    renderPanel();

    expect(await screen.findByText('report.docx')).toBeInTheDocument();
    expect(screen.getByText('budget.xlsx')).toBeInTheDocument();
    expect(screen.getByText('2.0 KB')).toBeInTheDocument();
    expect(screen.getByText('4.0 KB')).toBeInTheDocument();
    expect(mocked.getProjectDeliverables).toHaveBeenCalledWith('p1');
  });

  it('shows the empty hint when nothing was generated', async () => {
    renderPanel();
    expect(await screen.findByText(/No documents yet|还没有生成文档/)).toBeInTheDocument();
  });

  it('previews a Word document as paragraphs with its metadata', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(listing([entry({})]));
    mocked.getDeliverablePreview.mockResolvedValue(
      previewOf({
        docx: {
          title: 'Q3 report',
          author: 'Ada',
          paragraphs_total: 2,
          words: 5,
          paragraphs: ['Revenue grew', 'Costs fell'],
          truncated: false,
        },
      })
    );
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /report\.docx/ }));

    expect(await screen.findByText('Revenue grew')).toBeInTheDocument();
    expect(screen.getByText('Costs fell')).toBeInTheDocument();
    expect(screen.getByText('Q3 report')).toBeInTheDocument();
    expect(mocked.getDeliverablePreview).toHaveBeenCalledWith('p1', 'out/report.docx');
    expect(screen.getByText(/words|词/)).toBeInTheDocument();
  });

  it('previews a spreadsheet and switches between its sheets', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(
      listing([entry({ name: 'budget.xlsx', path: 'out/budget.xlsx', kind: 'xlsx' })])
    );
    mocked.getDeliverablePreview.mockResolvedValue(
      previewOf(
        {
          kind: 'xlsx',
          xlsx: {
            sheets_total: 2,
            truncated: false,
            sheets: [
              {
                name: 'Budget',
                rows: [
                  ['Item', 'Cost'],
                  ['Rent', '1200'],
                ],
                rows_total: 2,
                cols: 2,
                truncated: false,
              },
              { name: 'Notes', rows: [['done']], rows_total: 1, cols: 1, truncated: false },
            ],
          },
        },
        'budget.xlsx'
      )
    );
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /budget\.xlsx/ }));

    expect(await screen.findByText('1200')).toBeInTheDocument();
    expect(screen.getByText('Cost')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Notes' }));
    expect(await screen.findByText('done')).toBeInTheDocument();
  });

  it('previews a deck slide by slide', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(
      listing([entry({ name: 'deck.pptx', path: 'out/deck.pptx', kind: 'pptx' })])
    );
    mocked.getDeliverablePreview.mockResolvedValue(
      previewOf(
        {
          kind: 'pptx',
          pptx: {
            slides_total: 2,
            truncated: false,
            slides: [
              { index: 1, title: 'Intro', lines: ['welcome'] },
              { index: 2, title: 'Numbers', lines: ['revenue up'] },
            ],
          },
        },
        'deck.pptx'
      )
    );
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /deck\.pptx/ }));

    expect(await screen.findByText('Intro')).toBeInTheDocument();
    expect(screen.getByText('revenue up')).toBeInTheDocument();
    expect(screen.getByText(/Slide 1|第 1 页/)).toBeInTheDocument();
  });

  it('renders a PDF through an object URL', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(
      listing([entry({ name: 'chart.pdf', path: 'out/chart.pdf', kind: 'pdf' })])
    );
    mocked.getDeliverablePreview.mockResolvedValue(
      previewOf({ kind: 'pdf', passthrough: true }, 'chart.pdf')
    );
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /chart\.pdf/ }));

    expect(await screen.findByTitle('chart.pdf')).toBeInTheDocument();
    expect(mocked.downloadDeliverable).toHaveBeenCalledWith('p1', 'out/chart.pdf');
    expect(createObjectURL).toHaveBeenCalled();
  });

  it('exports the open document through a blob anchor', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(listing([entry({})]));
    mocked.getDeliverablePreview.mockResolvedValue(previewOf({ text: 'hello' }));
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /report\.docx/ }));
    expect(await screen.findByText('hello')).toBeInTheDocument();

    // The row has an icon-only export button; the modal header one carries the label.
    const headerExport = screen
      .getAllByRole('button', { name: /Export|导出/ })
      .find((button) => /^(Export|导出)$/.test(button.textContent?.trim() ?? ''));
    expect(headerExport).toBeDefined();
    fireEvent.click(headerExport!);

    expect(mocked.downloadDeliverable).toHaveBeenCalledWith('p1', 'out/report.docx');
    // The anchor is only clicked once the blob has been fetched.
    await waitFor(() => expect(HTMLAnchorElement.prototype.click).toHaveBeenCalled());
  });

  it('reports a document it cannot preview', async () => {
    mocked.getProjectDeliverables.mockResolvedValue(listing([entry({ path: 'out/broken.docx' })]));
    mocked.getDeliverablePreview.mockRejectedValue(new Error('not a zip archive'));
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /report\.docx/ }));

    expect(await screen.findByText(/cannot be previewed|无法预览/)).toBeInTheDocument();
    expect(screen.getByText('not a zip archive')).toBeInTheDocument();
  });
});
