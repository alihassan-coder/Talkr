import type { ReactNode } from 'react'
import { NavLink } from 'react-router'
import { Boxes, History, Mic, Settings, Volume2 } from 'lucide-react'
import { LogoMark } from '@/components/Logo'
import { Kbd } from '@/components/ui'
import { cx } from '@/lib/cx'

const primary = [
  { to: '/speak', label: 'Speak', icon: Volume2, key: '1' },
  { to: '/transcribe', label: 'Transcribe', icon: Mic, key: '2' },
  { to: '/models', label: 'Models', icon: Boxes, key: '3' },
  { to: '/history', label: 'History', icon: History, key: '4' },
]

function Item({ to, icon: Icon, label, shortcut }: { to: string; icon: typeof Mic; label: string; shortcut?: string }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        cx(
          'group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] transition-colors duration-200',
          isActive ? 'bg-fg/[0.08] text-fg' : 'text-fg/50 hover:bg-fg/[0.04] hover:text-fg/85',
        )
      }
    >
      <Icon className="size-4" strokeWidth={1.75} />
      <span className="flex-1">{label}</span>
      {shortcut ? (
        <span className="opacity-0 transition-opacity group-hover:opacity-100">
          <Kbd>Ctrl {shortcut}</Kbd>
        </span>
      ) : null}
    </NavLink>
  )
}

export function Sidebar({ footer }: { footer?: ReactNode }) {
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-fg/[0.08] bg-fg/[0.015] p-3">
      <div className="flex items-center gap-2 px-2 pb-5 pt-1.5">
        <LogoMark className="size-5" />
        <span className="text-sm font-semibold tracking-[-0.02em]">Talkr</span>
      </div>

      <nav aria-label="Main" className="space-y-0.5">
        {primary.map((item) => (
          <Item key={item.to} to={item.to} icon={item.icon} label={item.label} shortcut={item.key} />
        ))}
      </nav>

      <div className="mt-auto space-y-3">
        {footer}
        <Item to="/settings" icon={Settings} label="Settings" shortcut="," />
      </div>
    </aside>
  )
}
