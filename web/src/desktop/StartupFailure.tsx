/**
 * Shown when the desktop shell could not bring the local daemon up.
 *
 * This renders before `I18nProvider` exists (the UI was never mounted), so the copy is bilingual
 * and static instead of going through `t()`.
 */
export default function StartupFailure({ message }: { message: string }) {
  return (
    <div className="min-h-screen flex items-center justify-center bg-background text-text p-8">
      <div className="max-w-lg w-full space-y-4">
        <h1 className="text-xl font-semibold">Synbot 无法启动本地服务</h1>
        <p className="text-sm text-text-secondary">Synbot could not start its local daemon.</p>
        <pre className="text-xs whitespace-pre-wrap break-words bg-surface border border-border rounded p-3 text-error">
          {message}
        </pre>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="px-3 py-1.5 text-sm rounded border border-border bg-surface hover:text-primary"
        >
          重试 / Retry
        </button>
      </div>
    </div>
  );
}
