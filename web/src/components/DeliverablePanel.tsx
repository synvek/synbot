import { useEffect, useState } from 'react';
import { Download, RefreshCw, X } from 'lucide-react';
import { apiClient } from '../api';
import type {
  Deliverable,
  DeliverableKind,
  DeliverableList,
  DeliverablePreviewResponse,
  DocxPreview,
  PptxPreview,
  Project,
  XlsxPreview,
} from '../types/api';
import { useI18n } from '../i18n/I18nContext';
import { formatDateTime } from '../utils/datetime';

interface DeliverablePanelProps {
  project: Project;
}

/** Tabs of the deliverable preview, per kind. */
const KIND_BADGE: Record<DeliverableKind, string> = {
  docx: 'DOCX',
  xlsx: 'XLSX',
  pptx: 'PPTX',
  pdf: 'PDF',
  csv: 'CSV',
  markdown: 'MD',
  html: 'HTML',
  image: 'IMG',
  text: 'TXT',
};

function formatBytes(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function formatWhen(ms: number) {
  if (!ms) return '';
  const date = new Date(ms);
  return formatDateTime(date);
}

/**
 * "Deliverables" tab: the office documents an agent produced, with a text preview and an export
 * button. Text extraction happens on the server; PDFs and images are rendered by the browser.
 */
export default function DeliverablePanel({ project }: DeliverablePanelProps) {
  const { t } = useI18n();
  const [listing, setListing] = useState<DeliverableList | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const [openingPath, setOpeningPath] = useState<string | null>(null);
  const [opened, setOpened] = useState<DeliverablePreviewResponse | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [objectUrl, setObjectUrl] = useState<string | null>(null);
  const [sheetIndex, setSheetIndex] = useState(0);
  const [busyPath, setBusyPath] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);
    apiClient
      .getProjectDeliverables(project.id)
      .then((list) => {
        if (active) setListing(list);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [project.id, reloadToken]);

  // Object URLs pin the blob in memory until they are revoked.
  useEffect(() => {
    return () => {
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [objectUrl]);

  const closePreview = () => {
    setOpened(null);
    setPreviewError(null);
    setObjectUrl(null);
    setSheetIndex(0);
  };

  const openPreview = async (entry: Deliverable) => {
    setPreviewError(null);
    setOpened(null);
    setObjectUrl(null);
    setSheetIndex(0);
    setOpeningPath(entry.path);
    try {
      const response = await apiClient.getDeliverablePreview(project.id, entry.path);
      setOpened(response);
      if (response.preview.passthrough) {
        const blob = await apiClient.downloadDeliverable(project.id, entry.path);
        setObjectUrl(URL.createObjectURL(blob));
      }
    } catch (e) {
      setPreviewError(e instanceof Error ? e.message : String(e));
    } finally {
      setOpeningPath(null);
    }
  };

  const download = async (entry: { path: string; name: string }) => {
    setBusyPath(entry.path);
    setError(null);
    try {
      const blob = await apiClient.downloadDeliverable(project.id, entry.path);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = entry.name;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyPath(null);
    }
  };

  const entries = listing?.entries ?? [];
  const showPreview = opened !== null || previewError !== null || openingPath !== null;

  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-line text-[11px] text-ink-2">
        <span>
          {t('project.deliverableCount', { count: entries.length })}
        </span>
        {listing?.truncated && (
          <span className="text-error">{t('project.deliverableTruncated', '列表已截断')}</span>
        )}
        <button
          type="button"
          onClick={() => setReloadToken((value) => value + 1)}
          className="ml-auto p-1 rounded hover:text-ink"
          title={t('project.refresh', '刷新')}
        >
          <RefreshCw size={11} />
        </button>
      </div>

      {error && <div className="px-3 py-1 text-[11px] text-error break-all">{error}</div>}

      <ul className="flex-1 overflow-y-auto text-xs">
        {loading && entries.length === 0 && (
          <li className="px-3 py-2 text-ink-2">{t('common.loading')}</li>
        )}
        {!loading && entries.length === 0 && (
          <li className="px-3 py-2 text-ink-2">
            {t(
              'project.deliverableEmpty',
              '还没有生成文档。agent 写出 docx/xlsx/pptx/pdf 后会出现在这里。'
            )}
          </li>
        )}
        {entries.map((entry) => (
          <li key={entry.path} className="group flex items-center">
            <button
              type="button"
              onClick={() => void openPreview(entry)}
              disabled={openingPath === entry.path}
              className="flex-1 min-w-0 flex items-center gap-2 px-3 py-1 text-left hover:bg-surface-2 disabled:opacity-50"
              title={`${entry.path} — ${formatWhen(entry.modified_ms)}`}
            >
              <span className="shrink-0 w-9 text-[9px] font-semibold text-ink-2">
                {KIND_BADGE[entry.kind] ?? entry.kind.toUpperCase()}
              </span>
              <span className="truncate font-mono text-ink">{entry.name}</span>
              <span className="ml-auto shrink-0 text-[10px] text-ink-2">
                {formatBytes(entry.size)}
              </span>
            </button>
            <button
              type="button"
              onClick={() => void download(entry)}
              disabled={busyPath === entry.path}
              className="shrink-0 p-1 mr-1 rounded text-ink-2 opacity-0 group-hover:opacity-100 hover:text-ink disabled:opacity-50"
              title={t('project.deliverableDownload', '导出')}
            >
              <Download size={12} />
            </button>
          </li>
        ))}
      </ul>

      {showPreview && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
          <div className="flex flex-col w-full max-w-4xl max-h-[85vh] rounded-lg bg-surface border border-line overflow-hidden">
            <div className="flex items-center gap-2 px-3 py-2 border-b border-line">
              <span className="text-[9px] font-semibold text-ink-2 w-9 shrink-0">
                {opened ? KIND_BADGE[opened.preview.kind] ?? opened.preview.kind : ''}
              </span>
              <span className="text-sm font-mono text-ink truncate">
                {opened?.path ?? openingPath ?? ''}
              </span>
              {opened?.preview.truncated && (
                <span className="text-[10px] text-error shrink-0">
                  {t('project.deliverableClamped', '已截断，导出看完整内容')}
                </span>
              )}
              {opened && (
                <button
                  type="button"
                  onClick={() => void download({ path: opened.path, name: opened.name })}
                  disabled={busyPath === opened.path}
                  className="ml-auto flex items-center gap-1 px-1.5 py-0.5 rounded border border-line text-[11px] text-ink-2 hover:text-ink disabled:opacity-50"
                >
                  <Download size={11} />
                  {busyPath === opened.path
                    ? t('project.deliverableDownloading', '导出中…')
                    : t('project.deliverableDownload', '导出')}
                </button>
              )}
              <button
                type="button"
                onClick={closePreview}
                className={`${opened ? '' : 'ml-auto '}p-1 rounded text-ink-2 hover:text-ink`}
              >
                <X size={16} />
              </button>
            </div>

            {previewError ? (
              <div className="p-4 text-sm text-error">
                <div className="mb-1">{t('project.deliverablePreviewFailed', '无法预览该文件')}</div>
                <div className="text-xs text-ink-2 break-all">{previewError}</div>
              </div>
            ) : openingPath !== null ? (
              <div className="p-4 text-sm text-ink-2">
                {t('project.deliverableRendering', '正在渲染…')}
              </div>
            ) : opened ? (
              <PreviewBody
                opened={opened}
                objectUrl={objectUrl}
                sheetIndex={sheetIndex}
                onSheetChange={setSheetIndex}
              />
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}

interface PreviewBodyProps {
  opened: DeliverablePreviewResponse;
  objectUrl: string | null;
  sheetIndex: number;
  onSheetChange: (index: number) => void;
}

function PreviewBody({ opened, objectUrl, sheetIndex, onSheetChange }: PreviewBodyProps) {
  const { t } = useI18n();
  const { preview } = opened;

  if (preview.passthrough) {
    if (!objectUrl) {
      return (
        <div className="p-4 text-sm text-ink-2">
          {t('project.deliverableRendering', '正在渲染…')}
        </div>
      );
    }
    return preview.kind === 'image' ? (
      <div className="flex-1 overflow-auto bg-surface-2 flex items-center justify-center p-3">
        <img src={objectUrl} alt={opened.name} className="max-w-full max-h-[70vh] object-contain" />
      </div>
    ) : (
      <iframe src={objectUrl} title={opened.name} className="flex-1 w-full bg-surface-2 min-h-[70vh]" />
    );
  }

  if (preview.docx) return <DocxBody docx={preview.docx} />;
  if (preview.xlsx) {
    return <XlsxBody xlsx={preview.xlsx} index={sheetIndex} onIndex={onSheetChange} />;
  }
  if (preview.pptx) return <PptxBody pptx={preview.pptx} />;
  return (
    <pre className="flex-1 overflow-auto p-3 text-xs font-mono text-ink bg-surface-2 whitespace-pre-wrap break-words">
      {preview.text ?? ''}
    </pre>
  );
}

function DocxBody({ docx }: { docx: DocxPreview }) {
  const { t } = useI18n();
  return (
    <div className="flex-1 overflow-auto bg-surface-2">
      <div className="flex items-center gap-3 px-4 py-2 border-b border-line text-[11px] text-ink-2">
        {docx.title && <span className="text-ink truncate">{docx.title}</span>}
        {docx.author && <span>{docx.author}</span>}
        <span className="ml-auto shrink-0">
          {t('project.deliverableWords', { count: docx.words })}
        </span>
        <span className="shrink-0">
          {t('project.deliverableParagraphs', { count: docx.paragraphs_total })}
        </span>
      </div>
      <div className="p-4 space-y-2 text-sm text-ink leading-relaxed">
        {docx.paragraphs.map((paragraph, index) => (
          <p key={index} className="whitespace-pre-wrap break-words">
            {paragraph}
          </p>
        ))}
        {docx.paragraphs.length === 0 && (
          <p className="text-ink-2">{t('project.deliverableNoText', '文档里没有文字')}</p>
        )}
      </div>
    </div>
  );
}

function XlsxBody({
  xlsx,
  index,
  onIndex,
}: {
  xlsx: XlsxPreview;
  index: number;
  onIndex: (index: number) => void;
}) {
  const { t } = useI18n();
  const sheet = xlsx.sheets[Math.min(index, xlsx.sheets.length - 1)];
  if (!sheet) {
    return (
      <div className="p-4 text-sm text-ink-2">
        {t('project.deliverableNoText', '文档里没有文字')}
      </div>
    );
  }
  return (
    <div className="flex flex-col flex-1 min-h-0 bg-surface-2">
      <div className="flex items-center gap-1 px-2 py-1 border-b border-line text-[11px] overflow-x-auto">
        {xlsx.sheets.map((item, sheetIndex) => (
          <button
            key={`${item.name}-${sheetIndex}`}
            type="button"
            onClick={() => onIndex(sheetIndex)}
            className={`px-2 py-0.5 rounded shrink-0 ${
              sheetIndex === Math.min(index, xlsx.sheets.length - 1)
                ? 'bg-surface text-ink border border-line'
                : 'text-ink-2 hover:text-ink'
            }`}
          >
            {item.name}
          </button>
        ))}
        <span className="ml-auto shrink-0 text-ink-2">
          {t('project.deliverableRows', { count: sheet.rows_total })}
        </span>
      </div>
      <div className="flex-1 overflow-auto">
        <table className="text-xs border-collapse">
          <tbody>
            {sheet.rows.map((row, rowIndex) => (
              <tr key={rowIndex} className="even:bg-surface/40">
                <td className="px-1.5 py-0.5 text-right text-[10px] text-ink-2 select-none border-r border-b border-line/60">
                  {rowIndex + 1}
                </td>
                {row.map((cell, cellIndex) => (
                  <td
                    key={cellIndex}
                    className="px-2 py-0.5 text-ink border-r border-b border-line/60 whitespace-pre-wrap max-w-[24rem] truncate"
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {sheet.rows.length === 0 && (
          <div className="p-3 text-xs text-ink-2">
            {t('project.deliverableNoText', '文档里没有文字')}
          </div>
        )}
      </div>
    </div>
  );
}

function PptxBody({ pptx }: { pptx: PptxPreview }) {
  const { t } = useI18n();
  return (
    <div className="flex-1 overflow-auto bg-surface-2 p-3 space-y-3">
      <div className="text-[11px] text-ink-2">
        {t('project.deliverableSlides', { count: pptx.slides_total })}
      </div>
      {pptx.slides.map((slide) => (
        <section key={slide.index} className="rounded border border-line bg-surface p-3">
          <div className="flex items-baseline gap-2">
            <span className="text-[10px] text-ink-2 shrink-0">
              {t('project.deliverableSlide', { index: slide.index })}
            </span>
            <span className="text-sm font-semibold text-ink truncate">
              {slide.title ?? ''}
            </span>
          </div>
          {slide.lines.length > 0 && (
            <ul className="mt-1.5 space-y-0.5 text-xs text-ink-2">
              {slide.lines.map((line, index) => (
                <li key={index} className="whitespace-pre-wrap break-words">
                  {line}
                </li>
              ))}
            </ul>
          )}
        </section>
      ))}
      {pptx.slides.length === 0 && (
        <div className="text-xs text-ink-2">
          {t('project.deliverableNoText', '文档里没有文字')}
        </div>
      )}
    </div>
  );
}
