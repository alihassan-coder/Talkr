/** Mirrors `overlay::State` in src-tauri/src/dictation/overlay.rs. */
export type PillState =
  | { kind: 'hidden' }
  | { kind: 'listening'; session: number; locked: boolean; app: string | null }
  | { kind: 'working'; session: number; label: string }
  | { kind: 'done'; session: number; preview: string; app: string | null }
  | { kind: 'notice'; session: number; tone: 'info' | 'warn' | 'error'; title: string; detail: string | null }
  | { kind: 'cancelled'; session: number }

/** Short, spoken summary of the state for screen readers. */
export function announce(state: PillState): string {
  switch (state.kind) {
    case 'listening':
      return state.locked ? 'Dictating hands-free' : 'Listening'
    case 'working':
      return state.label
    case 'done':
      return `Inserted: ${state.preview}`
    case 'notice':
      return [state.title, state.detail].filter(Boolean).join('. ')
    case 'cancelled':
      return 'Cancelled'
    case 'hidden':
      return ''
  }
}

let pillUp = false
/** The overlay pill shows something now (its window may be up): the theme waits to reload until it goes. */
export const isPillUp = () => pillUp
export const notePillUp = (up: boolean) => {
  pillUp = up
}
