import { useEffect, useEffectEvent, useId, useLayoutEffect, useRef, useState } from 'react'
import type { KeyboardEvent as ReactKeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { Check, ChevronDown } from 'lucide-react'
import { cx } from '@/lib/cx'

export type SelectOption = { value: string; label: string; hint?: string; group?: string }

const GAP = 6
const EDGE = 8
const MAX_HEIGHT = 320

/**
 * Select-only combobox (WAI-ARIA 1.2 pattern). Focus stays on the trigger and
 * the active option is announced through aria-activedescendant. The list is
 * portalled to <body> with fixed positioning, so no scroll container or
 * transformed parent can clip it.
 */
export function Select({
  label,
  options,
  value,
  onChange,
  placeholder = 'Select',
  disabled = false,
  className,
  'aria-label': ariaLabel,
}: {
  /** Visible prefix inside the trigger. Pass '' and an aria-label when the row already names it. */
  label: string
  options: SelectOption[]
  value: string
  onChange: (value: string) => void
  /** Shown when `value` matches no option. */
  placeholder?: string
  disabled?: boolean
  className?: string
  'aria-label'?: string
}) {
  const id = useId()
  const listId = `${id}-list`
  const labelId = `${id}-label`
  const optionId = (i: number) => `${id}-opt-${i}`

  const triggerRef = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const typeahead = useRef({ text: '', timer: undefined as ReturnType<typeof setTimeout> | undefined })

  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(-1)
  const isOpen = open && !disabled

  const selectedIndex = options.findIndex((o) => o.value === value)
  const selected = selectedIndex >= 0 ? options[selectedIndex] : undefined

  const show = (index: number) => {
    setActive(index)
    setOpen(true)
  }
  const close = () => setOpen(false)

  const choose = (index: number) => {
    const option = options[index]
    close()
    if (option && option.value !== value) onChange(option.value)
  }

  // Places the list under the trigger, or above it when there is more room there.
  const place = () => {
    const trigger = triggerRef.current
    const list = listRef.current
    if (!trigger || !list) return
    const rect = trigger.getBoundingClientRect()
    // The trigger scrolled out of view: close rather than float detached.
    if (rect.bottom < 0 || rect.top > window.innerHeight) return close()

    const below = window.innerHeight - rect.bottom - GAP - EDGE
    const above = rect.top - GAP - EDGE
    const wanted = Math.min(list.scrollHeight, MAX_HEIGHT)
    const down = below >= wanted || below >= above
    const maxHeight = Math.min(MAX_HEIGHT, down ? below : above)

    list.style.minWidth = `${rect.width}px`
    list.style.maxHeight = `${Math.max(maxHeight, 80)}px`
    list.style.top = down ? `${rect.bottom + GAP}px` : ''
    list.style.bottom = down ? '' : `${window.innerHeight - rect.top + GAP}px`
    list.style.setProperty('--pop-from', down ? '-4px' : '4px')
    list.style.transformOrigin = down ? 'top' : 'bottom'
    const left = Math.min(rect.left, window.innerWidth - EDGE - list.offsetWidth)
    list.style.left = `${Math.max(EDGE, left)}px`
  }

  // Keyboard moves scroll the active option into view; the mouse must not, or the list drifts under it.
  const keyboardNav = useRef(false)

  const onOpened = useEffectEvent(() => {
    place()
    if (selectedIndex >= 0) document.getElementById(optionId(selectedIndex))?.scrollIntoView({ block: 'nearest' })
  })
  const reveal = useEffectEvent((index: number) => {
    if (keyboardNav.current) document.getElementById(optionId(index))?.scrollIntoView({ block: 'nearest' })
  })
  const onPointerDownAnywhere = useEffectEvent((e: PointerEvent) => {
    const target = e.target as Node
    if (triggerRef.current?.contains(target) || listRef.current?.contains(target)) return
    close()
  })
  const onScrollAnywhere = useEffectEvent((e: Event) => {
    // Scrolling the list itself is fine; any other scroll may have moved the trigger.
    if (e.target instanceof Node && listRef.current?.contains(e.target)) return
    place()
  })

  useLayoutEffect(() => {
    if (isOpen) onOpened()
  }, [isOpen])

  useEffect(() => {
    if (isOpen && active >= 0) reveal(active)
  }, [isOpen, active])

  useEffect(() => {
    if (!isOpen) return
    const onPointerDown = (e: PointerEvent) => onPointerDownAnywhere(e)
    const onScroll = (e: Event) => onScrollAnywhere(e)
    const onDismiss = () => setOpen(false)
    document.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('scroll', onScroll, true)
    window.addEventListener('resize', onDismiss)
    window.addEventListener('blur', onDismiss)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('scroll', onScroll, true)
      window.removeEventListener('resize', onDismiss)
      window.removeEventListener('blur', onDismiss)
    }
  }, [isOpen])

  /** Next option whose label starts with the typed letters, cycling from the current one. */
  const findByText = (text: string, from: number) => {
    const needle = text.toLowerCase()
    const start = text.length === 1 ? from + 1 : Math.max(from, 0)
    for (let step = 0; step < options.length; step++) {
      const i = (start + step) % options.length
      if (options[i]?.label.toLowerCase().startsWith(needle)) return i
    }
    return -1
  }

  const onKeyDown = (e: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (disabled || options.length === 0) return
    keyboardNav.current = true
    const last = options.length - 1
    const current = isOpen ? active : selectedIndex

    const isChar = e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey
    // A space in the middle of type-ahead is part of the search, not a selection.
    if (isChar && (e.key !== ' ' || typeahead.current.text !== '')) {
      const t = typeahead.current
      clearTimeout(t.timer)
      t.text += e.key
      t.timer = setTimeout(() => (t.text = ''), 600)
      const match = findByText(t.text, current)
      if (match >= 0) show(match)
      else if (!isOpen) show(Math.max(selectedIndex, 0))
      e.preventDefault()
      return
    }

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        if (!isOpen) show(Math.max(selectedIndex, 0))
        else setActive(Math.min(last, active + 1))
        return
      case 'ArrowUp':
        e.preventDefault()
        if (!isOpen) show(Math.max(selectedIndex, 0))
        else setActive(Math.max(0, active - 1))
        return
      case 'Home':
      case 'PageUp':
        e.preventDefault()
        show(0)
        return
      case 'End':
      case 'PageDown':
        e.preventDefault()
        show(last)
        return
      case 'Enter':
      case ' ':
        e.preventDefault()
        if (!isOpen) show(Math.max(selectedIndex, 0))
        else if (active >= 0) choose(active)
        return
      case 'Escape':
        if (!isOpen) return
        e.preventDefault()
        e.stopPropagation()
        close()
        return
      case 'Tab':
        if (isOpen) close()
        return
    }
  }

  // Consecutive options that share a `group` are rendered under one header.
  const sections: { group: string | undefined; items: { option: SelectOption; index: number }[] }[] = []
  options.forEach((option, index) => {
    const tail = sections[sections.length - 1]
    if (tail && tail.group === option.group) tail.items.push({ option, index })
    else sections.push({ group: option.group, items: [{ option, index }] })
  })

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        aria-controls={listId}
        aria-activedescendant={isOpen && active >= 0 ? optionId(active) : undefined}
        aria-label={label ? undefined : ariaLabel}
        aria-labelledby={label ? labelId : undefined}
        disabled={disabled}
        onClick={() => (isOpen ? close() : show(Math.max(selectedIndex, 0)))}
        onKeyDown={onKeyDown}
        className={cx(
          'relative inline-flex h-9 min-w-0 items-center rounded-full border pl-3.5 pr-8 text-left text-[13px] outline-none transition-colors duration-200 focus-visible:border-fg/40 disabled:cursor-not-allowed disabled:opacity-40',
          isOpen ? 'border-fg/30 bg-fg/[0.03]' : 'border-fg/12 enabled:hover:border-fg/20',
          className,
        )}
      >
        {label ? (
          <span id={labelId} className="mr-1.5 shrink-0 text-fg/45">
            {label}
          </span>
        ) : null}
        <span className={cx('min-w-0 truncate', selected ? 'font-medium text-fg' : 'text-fg/45')}>
          {selected ? selected.label : placeholder}
        </span>
        <ChevronDown
          aria-hidden="true"
          className={cx(
            'pointer-events-none absolute right-3 size-3.5 text-fg/45 transition-transform duration-250 ease-out-quint',
            isOpen && 'rotate-180',
          )}
          strokeWidth={2}
        />
      </button>

      {isOpen
        ? createPortal(
            <div
              ref={listRef}
              id={listId}
              role="listbox"
              aria-label={ariaLabel ?? label}
              tabIndex={-1}
              // Keep focus on the trigger while clicking inside the list.
              onMouseDown={(e) => e.preventDefault()}
              className="fixed z-50 max-w-[min(360px,calc(100vw-16px))] animate-pop overflow-y-auto overscroll-contain rounded-xl border border-fg/[0.1] bg-bg p-1 text-[13px] shadow-[var(--shadow-pop)] outline-none"
            >
              {options.length === 0 ? <p className="px-2.5 py-1.5 text-fg/40">No options</p> : null}
              {sections.map((section, s) => (
                <div
                  key={`${section.group ?? ''}-${s}`}
                  role={section.group ? 'group' : undefined}
                  aria-labelledby={section.group ? `${id}-group-${s}` : undefined}
                  className={cx(s > 0 && section.group && 'mt-1 border-t border-fg/[0.07] pt-1')}
                >
                  {section.group ? (
                    <p
                      id={`${id}-group-${s}`}
                      className="px-2.5 pb-1 pt-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-fg/40"
                    >
                      {section.group}
                    </p>
                  ) : null}
                  {section.items.map(({ option, index }) => {
                    const isSelected = index === selectedIndex
                    return (
                      <div
                        key={`${option.value}-${index}`}
                        id={optionId(index)}
                        role="option"
                        aria-selected={isSelected}
                        onMouseMove={() => {
                          keyboardNav.current = false
                          if (active !== index) setActive(index)
                        }}
                        onClick={() => choose(index)}
                        className={cx(
                          'flex cursor-pointer items-center gap-3 whitespace-nowrap rounded-lg py-1.5 pl-2.5 pr-2 transition-colors duration-100',
                          index === active && 'bg-fg/[0.07]',
                          isSelected ? 'font-medium text-fg' : 'text-fg/70',
                        )}
                      >
                        <span className="min-w-0 flex-1 truncate">{option.label}</span>
                        {option.hint ? (
                          <span className="shrink-0 font-mono text-[11px] font-normal text-fg/40">{option.hint}</span>
                        ) : null}
                        <Check
                          aria-hidden="true"
                          className={cx('size-3.5 shrink-0', isSelected ? 'opacity-100' : 'opacity-0')}
                          strokeWidth={2.25}
                        />
                      </div>
                    )
                  })}
                </div>
              ))}
            </div>,
            document.body,
          )
        : null}
    </>
  )
}
