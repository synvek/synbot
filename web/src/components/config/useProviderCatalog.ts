import { useEffect, useState } from 'react'
import { apiClient } from '../../api'
import type { ProviderCatalog } from '../../types/api'

// One request per page load: the settings form re-renders on every keystroke, while the catalog
// only changes when config is saved (or the daemon restarts). A failed request resets the cache so
// reopening the page retries instead of pinning an empty result.
let pending: Promise<ProviderCatalog | null> | null = null

function load(): Promise<ProviderCatalog | null> {
  if (!pending) {
    pending = apiClient.listProviders().catch(() => {
      pending = null
      return null
    })
  }
  return pending
}

/**
 * Provider metadata from `GET /api/providers`.
 *
 * Returns `null` while loading and when the endpoint is unavailable. Callers must keep working in
 * that case — the settings form still has to edit `apiKey` / `apiBase` for an older daemon.
 */
export function useProviderCatalog(): ProviderCatalog | null {
  const [catalog, setCatalog] = useState<ProviderCatalog | null>(null)

  useEffect(() => {
    let alive = true
    load().then((next) => {
      if (alive) setCatalog(next)
    })
    return () => {
      alive = false
    }
  }, [])

  return catalog
}

/** Test hook: drop the module-level cache between renders. */
export function resetProviderCatalogCache(): void {
  pending = null
}
