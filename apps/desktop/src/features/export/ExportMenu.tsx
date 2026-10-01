import { useEffect, useEffectEvent, useId, useLayoutEffect, useRef, useState } from 'react'
import type { KeyboardEvent as ReactKeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { ChevronDown, Download } from 'lucide-react'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import type { HistoryItem, SaveFormat } from '@/lib/types'
import { isTauri } from '@/lib/api'
import { toast, toastError } from '@/stores/toast'
import { useUi } from '@/stores/ui'
import { formatLabel, formatsFor, saveItem, type FormatInfo } from './formats'

const GAP = 6
const EDGE = 8

/**
 * "Save as …" split button: the main part saves in the format used last (audio first when the
 * item has audio), the chevron opens every format the item supports. Follows the WAI-ARIA menu
 * button pattern: arrows move between items, Escape closes and returns focus.
 */
export function ExportMenu({ item, prefer = 'audio' }: { item: HistoryItem; prefer?: 'audio' | 'text' }) {
  const id = useId()
  const menuId = `${id}-menu`
  const { audio, text } = formatsFor(item)
  const options: FormatInfo[] = [...audio, ...text]
  const audioFormat = useUi((s) => s.audioFormat)
  const textFormat = useUi((s) => s.textFormat)
  const rememberFormat = useUi((s) => s.rememberFormat)

  const preferred = prefer === 'audio' && audio.length ? audioFormat : textFormat
  const quick = options.find((o) => o.format === preferred) ?? options[0]

  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [busy, setBusy] = useState<{ format: SaveFormat; progress: number | null } | null>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([])

  const close = (focusTrigger = false) => {
    setOpen(false)
    if (focusTrigger) triggerRef.current?.focus()
  }

  const save = async (format: SaveFormat) => {
    close()
    if (!isTauri()) return toast('Saving files is available in the desktop app')
    setBusy({ format, progress: format === 'mp3' ? 0 : null })
    try {
      const saved = await saveItem(item, format, (progress) => setBusy({ format, progress }))
      rememberFormat(format)
      if (saved) toast(`Saved as ${formatLabel(format)}`)
    } catch (err) {
      toastError(err)
    } finally {
      setBusy(null)
    }
  }

  const place = useEffectEvent(() => {
    const trigger = triggerRef.current
    const menu = menuRef.current
    if (!trigger || !menu) return
    const rect = trigger.getBoundingClientRect()
    const below = window.innerHeight - rect.bottom - GAP - EDGE
    const down = below >= menu.offsetHeight || below >= rect.top
    menu.style.top = down ? `${rect.bottom + GAP}px` : ''
    menu.style.bottom = down ? '' : `${window.innerHeight - rect.top + GAP}px`
    menu.style.setProperty('--pop-from', down ? '-4px' : '4px')
    menu.style.transformOrigin = down ? 'top right' : 'bottom right'
    // Right-aligned with the button, kept inside the window.
    const left = Math.min(rect.right - menu.offsetWidth, window.innerWidth - EDGE - menu.offsetWidth)
    menu.style.left = `${Math.max(EDGE, left)}px`
  })

  useLayoutEffect(() => {
    if (!open) return
    place()
    itemRefs.current[active]?.focus()
  }, [open, active])

  const onPointerDownAnywhere = useEffectEvent((e: PointerEvent) => {
    const target = e.target as Node
    if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return
    close()
  })

  useEffect(() => {
    if (!open) return
    const onPointerDown = (e: PointerEvent) => onPointerDownAnywhere(e)
    const onDismiss = () => setOpen(false)
    document.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('resize', onDismiss)
    window.addEventListener('blur', onDismiss)
    window.addEventListener('scroll', onDismiss, true)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('resize', onDismiss)
      window.removeEventListener('blur', onDismiss)
      window.removeEventListener('scroll', onDismiss, true)
    }
  }, [open])

  const openAt = (index: number) => {
    setActive(index)
    setOpen(true)
  }

  const onTriggerKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      openAt(e.key === 'ArrowDown' ? 0 : options.length - 1)
    }
  }

  const onMenuKeyDown = (e: ReactKeyboardEvent) => {
    const last = options.length - 1
    const moves: Record<string, number> = {
      ArrowDown: active >= last ? 0 : active + 1,
      ArrowUp: active <= 0 ? last : active - 1,
      Home: 0,
      End: last,
    }
    if (e.key in moves) {
      e.preventDefault()
      setActive(moves[e.key]!)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      close(true)
    } else if (e.key === 'Tab') {
      close()
    }
  }

  if (!quick) return null

  const busyLabel =
    busy && busy.progress !== null
      ? `Encoding ${formatLabel(busy.format)}… ${Math.round(busy.progress * 100)}%`
      : busy
        ? 'Saving…'
        : null

  const groups = [
    { label: 'Audio', items: audio },
    { label: 'Text', items: text },
  ].filter((g) => g.items.length)

  return (
    <div className="inline-flex items-center">
      <Button
        size="sm"
        loading={busy !== null}
        icon={<Download className="size-3.5" strokeWidth={2} />}
        className="rounded-r-none border-r-0 pr-2.5"
        onClick={() => void save(quick.format)}
        aria-live="polite"
      >
        {busyLabel ?? `Save ${formatLabel(quick.format)}`}
      </Button>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label="More formats"
        title="More formats"
        disabled={busy !== null}
        onClick={() => (open ? close() : openAt(Math.max(0, options.indexOf(quick))))}
        onKeyDown={onTriggerKeyDown}
        className={cx(
          'grid h-8 w-8 place-items-center rounded-r-full border border-line text-muted transition-colors duration-200 hover:border-line-strong hover:bg-fg/[0.05] hover:text-fg disabled:cursor-not-allowed disabled:opacity-40',
          open && 'border-line-strong bg-fg/[0.05] text-fg',
        )}
      >
        <ChevronDown className={cx('size-3.5 transition-transform duration-200', open && 'rotate-180')} strokeWidth={2} />
      </button>

      {open
        ? createPortal(
            <div
              ref={menuRef}
              id={menuId}
              role="menu"
              aria-label="Save as"
              onKeyDown={onMenuKeyDown}
              className="fixed z-50 w-64 animate-pop rounded-xl border border-line bg-surface p-1 text-[13px] shadow-[var(--shadow-pop)]"
            >
              {groups.map((group, g) => (
                <div key={group.label} role="group" aria-label={group.label} className={cx(g > 0 && 'mt-1 border-t border-line pt-1')}>
                  <p aria-hidden="true" className="px-2.5 pb-1 pt-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-subtle">
                    {group.label}
                  </p>
                  {group.items.map((option) => {
                    const index = options.indexOf(option)
                    return (
                      <button
                        key={option.format}
                        ref={(el) => {
                          itemRefs.current[index] = el
                        }}
                        type="button"
                        role="menuitem"
                        tabIndex={index === active ? 0 : -1}
                        onMouseMove={() => index !== active && setActive(index)}
                        onClick={() => void save(option.format)}
                        className={cx(
                          'flex w-full items-baseline justify-between gap-3 rounded-lg px-2.5 py-1.5 text-left outline-none transition-colors duration-100',
                          index === active ? 'bg-fg/[0.06] text-fg' : 'text-muted',
                        )}
                      >
                        <span className={cx(option.format === quick.format && 'font-medium')}>{option.label}</span>
                        <span className="truncate font-mono text-[11px] text-subtle">{option.description}</span>
                      </button>
                    )
                  })}
                </div>
              ))}
            </div>,
            document.body,
          )
        : null}
    </div>
  )
}
