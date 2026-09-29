import type { ReactNode } from 'react'
import { NavLink } from 'react-router'
import { Boxes, History, Mic, PanelLeftClose, PanelLeftOpen, Settings, Volume2 } from 'lucide-react'
import { LogoMark } from '@/components/Logo'
import { Kbd } from '@/components/ui'
import { cx } from '@/lib/cx'
import { useUi } from '@/stores/ui'

const primary = [
  { to: '/speak', label: 'Speak', icon: Volume2, key: '1' },
  { to: '/transcribe', label: 'Transcribe', icon: Mic, key: '2' },
  { to: '/models', label: 'Models', icon: Boxes, key: '3' },
  { to: '/history', label: 'History', icon: History, key: '4' },
]

/**
 * Label to the right of an icon-only control. Pure CSS: shows on hover or
 * keyboard focus of the nearest `group/tip` parent, after a short delay.
 */
function Tip({ label, shortcut }: { label: string; shortcut?: string }) {
  return (
    <span
      role="tooltip"
      className="pointer-events-none absolute left-full top-1/2 z-30 ml-3 flex -translate-x-1 -translate-y-1/2 items-center gap-2 whitespace-nowrap rounded-lg bg-fg py-1 pl-2.5 pr-2 text-[12px] font-medium text-bg opacity-0 shadow-[var(--shadow-pop)] transition-[opacity,transform] duration-150 ease-out-quint group-hover/tip:translate-x-0 group-hover/tip:opacity-100 group-hover/tip:delay-300 group-focus-visible/tip:translate-x-0 group-focus-visible/tip:opacity-100"
    >
      {label}
      {shortcut ? <span className="font-mono text-[10.5px] font-normal text-bg/55">{shortcut}</span> : null}
    </span>
  )
}

function Item({
  to,
  icon: Icon,
  label,
  shortcut,
  collapsed,
}: {
  to: string
  icon: typeof Mic
  label: string
  shortcut?: string
  collapsed: boolean
}) {
  return (
    <NavLink
      to={to}
      aria-label={collapsed ? label : undefined}
      className={({ isActive }) =>
        cx(
          'group/tip relative flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[13px] transition-colors duration-200',
          isActive ? 'bg-fg/[0.07] font-medium text-fg' : 'text-muted hover:bg-fg/[0.045] hover:text-fg',
        )
      }
    >
      {/* Accent marker on the active item. */}
      <span
        aria-hidden="true"
        className="absolute inset-y-2.5 left-0 w-[3px] scale-y-0 rounded-r-full bg-accent transition-transform duration-250 ease-out-quint group-aria-[current=page]/tip:scale-y-100"
      />
      <Icon className="size-4 shrink-0 group-aria-[current=page]/tip:text-accent" strokeWidth={1.75} />
      {/* Clipped rather than reflowed while the width animates, so nothing wraps mid-transition. */}
      <span
        className={cx(
          'min-w-0 flex-1 overflow-hidden whitespace-nowrap transition-opacity duration-200',
          collapsed && 'opacity-0',
        )}
      >
        {label}
      </span>
      {shortcut && !collapsed ? (
        <span className="shrink-0 whitespace-nowrap opacity-0 transition-opacity group-hover/tip:opacity-100">
          <Kbd>Ctrl {shortcut}</Kbd>
        </span>
      ) : null}
      {collapsed ? <Tip label={label} shortcut={shortcut ? `Ctrl ${shortcut}` : undefined} /> : null}
    </NavLink>
  )
}

export function Sidebar({ footer }: { footer?: ReactNode }) {
  const collapsed = useUi((s) => s.sidebarCollapsed)
  const toggle = useUi((s) => s.toggleSidebar)

  return (
    <aside
      className={cx(
        'relative z-20 flex shrink-0 flex-col border-r border-line bg-panel p-3 transition-[width] duration-250 ease-out-quint',
        collapsed ? 'w-[60px]' : 'w-56',
      )}
    >
      <div className="group/head relative mb-4 flex h-8 items-center gap-2 px-2">
        <LogoMark className={cx('size-5 shrink-0 transition-opacity duration-150', collapsed && 'group-hover/head:opacity-0 group-has-focus-visible/head:opacity-0')} />
        <span
          className={cx(
            'overflow-hidden whitespace-nowrap text-sm font-semibold tracking-[-0.02em] transition-opacity duration-200',
            collapsed && 'opacity-0',
          )}
        >
          Talkr
        </span>

        {collapsed ? (
          // Collapsed: the logo slot doubles as the expand button.
          <button
            type="button"
            onClick={toggle}
            aria-label="Expand sidebar"
            aria-expanded={false}
            className="group/tip absolute inset-y-0 left-0 grid w-9 place-items-center rounded-lg text-muted transition-colors duration-200 hover:bg-fg/[0.06] hover:text-fg"
          >
            <span className="opacity-0 transition-opacity duration-150 group-hover/tip:opacity-100 group-focus-visible/tip:opacity-100">
              <PanelLeftOpen className="size-4" strokeWidth={1.75} />
            </span>
            <Tip label="Expand sidebar" shortcut="Ctrl B" />
          </button>
        ) : (
          <button
            type="button"
            onClick={toggle}
            aria-label="Collapse sidebar"
            aria-expanded={true}
            className="group/tip absolute right-0 grid size-8 place-items-center rounded-lg text-subtle transition-colors duration-200 hover:bg-fg/[0.06] hover:text-fg"
          >
            <PanelLeftClose className="size-4" strokeWidth={1.75} />
            <Tip label="Collapse sidebar" shortcut="Ctrl B" />
          </button>
        )}
      </div>

      <nav aria-label="Main" className="space-y-0.5">
        {primary.map((item) => (
          <Item key={item.to} to={item.to} icon={item.icon} label={item.label} shortcut={item.key} collapsed={collapsed} />
        ))}
      </nav>

      <div className="mt-auto space-y-3">
        {footer ? (
          // Fixed width inside a clipping box: the card fades out instead of squashing.
          <div
            inert={collapsed}
            aria-hidden={collapsed}
            className={cx(
              'overflow-hidden transition-[opacity,max-height] duration-250 ease-out-quint',
              collapsed ? 'max-h-0 opacity-0' : 'max-h-40 opacity-100',
            )}
          >
            <div className="w-50">{footer}</div>
          </div>
        ) : null}
        <Item to="/settings" icon={Settings} label="Settings" shortcut="," collapsed={collapsed} />
      </div>
    </aside>
  )
}
