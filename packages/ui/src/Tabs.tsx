"use client"

import { createContext, useContext, useState, ReactNode, HTMLAttributes } from 'react'

interface TabsContextValue {
  activeTab: string
  onTabChange: (tab: string) => void
}

const TabsContext = createContext<TabsContextValue | null>(null)

interface TabsProps {
  defaultTab?: string
  children: ReactNode
  className?: string
}

export function Tabs({ defaultTab, children, className = '' }: TabsProps) {
  const [activeTab, setActiveTab] = useState(defaultTab || '')

  return (
    <TabsContext.Provider value={{ activeTab, onTabChange: setActiveTab }}>
      <div className={className}>{children}</div>
    </TabsContext.Provider>
  )
}

interface TabListProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode
}

export function TabList({ children, className = '', ...props }: TabListProps) {
  return (
    <div role="tablist" className={`flex gap-1 bg-app-canvas rounded-[var(--radius-input)] p-1 ${className}`} {...props}>
      {children}
    </div>
  )
}

interface TabProps extends HTMLAttributes<HTMLButtonElement> {
  value: string
  disabled?: boolean
}

export function Tab({ value, disabled, children, className = '', ...props }: TabProps) {
  const context = useContext(TabsContext)
  if (!context) throw new Error('Tab must be used within Tabs')

  const { activeTab, onTabChange } = context
  const isActive = activeTab === value

  return (
    <button
      role="tab"
      aria-selected={isActive}
      aria-controls={`${value}-panel`}
      id={`${value}-tab`}
      onClick={() => !disabled && onTabChange(value)}
      disabled={disabled}
      className={`
        px-4 py-2 text-sm font-medium rounded-[var(--radius-pill)]
        transition-all duration-fast
        ${isActive
          ? 'bg-white text-text-primary shadow-[var(--shadow-card)]'
          : 'text-text-secondary hover:text-text-primary hover:bg-white/50'}
        ${disabled ? 'opacity-50 cursor-not-allowed' : ''}
        ${className}
      `}
      {...props}
    >
      {children}
    </button>
  )
}

interface TabPanelProps extends HTMLAttributes<HTMLDivElement> {
  value: string
}

export function TabPanel({ value, children, className = '', ...props }: TabPanelProps) {
  const context = useContext(TabsContext)
  if (!context) throw new Error('TabPanel must be used within Tabs')

  const { activeTab } = context

  if (activeTab !== value) return null

  return (
    <div
      role="tabpanel"
      id={`${value}-panel`}
      aria-labelledby={`${value}-tab`}
      className={`mt-4 ${className}`}
      {...props}
    >
      {children}
    </div>
  )
}