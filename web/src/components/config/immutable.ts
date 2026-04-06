/** Matches backend CONFIG_SECRET_MASK (see src/web/handlers/sanitize.rs). */
export const CONFIG_SECRET_MASK = '********'

export type PathSeg = string | number

/**
 * Deep-clone then set a value at a path (objects and arrays).
 */
export function setAtPath(root: Record<string, unknown>, path: PathSeg[], value: unknown): Record<string, unknown> {
  const o = structuredClone(root) as Record<string, unknown>
  if (path.length === 0) return o
  let cur: unknown = o
  for (let i = 0; i < path.length - 1; i++) {
    const k = path[i]
    const nextK = path[i + 1]
    if (typeof k === 'number') {
      const arr = cur as unknown[]
      if (!Array.isArray(arr)) throw new Error('setAtPath: expected array')
      while (arr.length <= k) arr.push(null)
      if (arr[k] === undefined || arr[k] === null) {
        arr[k] = typeof nextK === 'number' ? [] : {}
      }
      cur = arr[k]
    } else {
      const obj = cur as Record<string, unknown>
      if (obj[k] === undefined || obj[k] === null) {
        obj[k] = typeof nextK === 'number' ? [] : {}
      }
      cur = obj[k]
    }
  }
  const last = path[path.length - 1]
  if (typeof last === 'number') {
    const arr = cur as unknown[]
    if (!Array.isArray(arr)) throw new Error('setAtPath: expected array at leaf')
    while (arr.length <= last) arr.push(null)
    arr[last] = value
  } else {
    (cur as Record<string, unknown>)[last] = value
  }
  return o
}

export function getAtPath(obj: unknown, path: PathSeg[]): unknown {
  let cur: unknown = obj
  for (const k of path) {
    if (cur === null || cur === undefined) return undefined
    if (typeof k === 'number') {
      if (!Array.isArray(cur)) return undefined
      cur = cur[k]
    } else {
      if (typeof cur !== 'object') return undefined
      cur = (cur as Record<string, unknown>)[k]
    }
  }
  return cur
}

export function asRecord(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {}
}

export function asArray<T = unknown>(v: unknown): T[] {
  return Array.isArray(v) ? (v as T[]) : []
}
