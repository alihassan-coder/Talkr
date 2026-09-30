import { useEffect, useState } from 'react'
import { getEngineStatus, isTauri } from '@/lib/api'
import type { EngineStatus } from '@/lib/types'

export type Load = { state: 'loading' } | { state: 'ready'; status: EngineStatus } | { state: 'error' } | { state: 'preview' }

/** get_engine_status, refetched on demand (after the device setting changes). */
export function useEngineStatus() {
  const [load, setLoad] = useState<Load>(() => (isTauri() ? { state: 'loading' } : { state: 'preview' }))

  const refresh = async () => {
    if (!isTauri()) return
    try {
      setLoad({ state: 'ready', status: await getEngineStatus() })
    } catch {
      setLoad({ state: 'error' })
    }
  }

  useEffect(() => {
    if (!isTauri()) return
    let alive = true
    getEngineStatus()
      .then((status) => alive && setLoad({ state: 'ready', status }))
      .catch(() => alive && setLoad({ state: 'error' }))
    return () => {
      alive = false
    }
  }, [])

  return { load, refresh }
}

