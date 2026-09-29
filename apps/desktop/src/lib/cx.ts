/** Joins truthy class names. */
export const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ')
