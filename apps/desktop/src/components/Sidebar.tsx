import { NavLink } from 'react-router-dom'
import { Mic, Mic2, History, Download, Settings, Volume2, ChevronLeft } from 'lucide-react'

const navItems = [
  { path: '/speak', label: 'Speak', icon: Volume2 },
  { path: '/transcribe', label: 'Transcribe', icon: Mic },
  { path: '/history', label: 'History', icon: History },
  { path: '/models', label: 'Models', icon: Download },
] as const

export function Sidebar() {
  return (
    <aside className="w-60 bg-card border-r border-border flex flex-col h-full">
      <div className="p-6 border-b border-border">
        <NavLink to="/speak" className="flex items-center gap-3 text-text-primary">
          <Volume2 className="w-8 h-8 text-accent" strokeWidth={2} />
          <span className="text-xl font-semibold tracking-tight">Talkr</span>
        </NavLink>
      </div>

      <nav className="flex-1 p-4 space-y-1 overflow-y-auto">
        {navItems.map(({ path, label, icon: Icon }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              `flex items-center gap-3 px-3 py-2.5 rounded-[var(--radius-input)] text-text-secondary transition-colors ${
                isActive
                  ? 'bg-accent-soft text-accent font-medium'
                  : 'hover:bg-app-canvas hover:text-text-primary'
              }`
            }
          >
            <Icon className="w-5 h-5 stroke-[1.75] flex-shrink-0" />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>

      <div className="p-4 border-t border-border">
        <NavLink
          to="/settings"
          className="flex items-center gap-3 px-3 py-2.5 rounded-[var(--radius-input)] text-text-secondary transition-colors hover:bg-app-canvas hover:text-text-primary"
        >
          <Settings className="w-5 h-5 stroke-[1.75] flex-shrink-0" />
          <span>Settings</span>
        </NavLink>
      </div>
    </aside>
  )
}