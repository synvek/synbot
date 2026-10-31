/**
 * First paint of the desktop window, shown while `desktop_bootstrap` waits for the sidecar.
 *
 * The window now loads bundled assets instead of a daemon-hosted loading page, so the shell needs
 * its own "something is happening" screen — otherwise the user stares at a white window for the
 * tens of seconds a cold `synbot start` can take.
 */
export default function StartupSplash() {
  return (
    <div className="min-h-screen flex flex-col items-center justify-center gap-4 bg-surface-2 text-ink">
      <div
        className="w-8 h-8 rounded-full border-[3px] border-line border-t-primary animate-spin"
        aria-hidden
      />
      <div className="text-center space-y-1">
        <p className="font-semibold">Synbot</p>
        <p className="text-sm text-ink-2">正在启动本地服务… / Starting the local daemon…</p>
      </div>
    </div>
  );
}
