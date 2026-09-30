const FALLBACK = 'Something went wrong.'

const pick = (value: unknown): string | null => {
  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>
    for (const key of ['message', 'error', 'reason']) {
      const field = record[key]
      if (typeof field === 'string' && field.trim()) return field.trim()
      // `{ error: { message } }` and similar nesting.
      if (field && typeof field === 'object') {
        const nested = pick(field)
        if (nested) return nested
      }
    }
  }
  return null
}

/**
 * Readable text for anything a command or event can fail with. The backend sends plain
 * strings, but a plugin or a future change may send an object or a JSON-encoded one:
 * those are unwrapped to their message instead of being shown as raw JSON.
 */
export function errorText(error: unknown): string {
  if (typeof error === 'string') {
    const text = error.trim()
    if (!text) return FALLBACK
    if (text.startsWith('{')) {
      try {
        return pick(JSON.parse(text)) ?? FALLBACK
      } catch {
        return text
      }
    }
    return text
  }
  if (error instanceof Error) return error.message.trim() || FALLBACK
  return pick(error) ?? FALLBACK
}
