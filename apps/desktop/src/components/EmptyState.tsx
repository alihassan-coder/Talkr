import { Link } from 'react-router-dom'
import { ExternalLink } from 'lucide-react'

interface EmptyStateProps {
  title: string
  description: string
  actionLabel?: string
  actionHref?: string
}

export function EmptyState({ title, description, actionLabel, actionHref }: EmptyStateProps) {
  return (
    <div className="bg-card rounded-[var(--radius-card)] p-12 shadow-[var(--shadow-card)] border border-border text-center">
      <div className="w-16 h-16 rounded-full bg-accent-soft flex items-center justify-center mx-auto mb-6 text-accent">
        <ExternalLink className="w-8 h-8 stroke-[1.75]" />
      </div>
      <h2 className="text-xl font-medium text-text-primary mb-2">{title}</h2>
      <p className="text-text-secondary mb-6 max-w-md mx-auto">{description}</p>
      {actionLabel && actionHref && (
        <Link
          to={actionHref}
          className="inline-flex items-center gap-2 px-5 py-2.5 bg-accent text-white rounded-[var(--radius-pill)] font-medium text-sm hover:bg-accent-hover transition-colors"
        >
          {actionLabel}
        </Link>
      )}
    </div>
  )
}