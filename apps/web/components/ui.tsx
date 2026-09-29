import type { ReactNode } from 'react'

export function Container({ className = '', children }: { className?: string; children: ReactNode }) {
  return <div className={`mx-auto w-full max-w-6xl px-5 md:px-8 ${className}`}>{children}</div>
}

/** Centered section intro: small mono kicker, headline, optional lede. */
export function SectionIntro({ kicker, title, lede }: { kicker: string; title: ReactNode; lede?: ReactNode }) {
  return (
    <div className="reveal mx-auto max-w-2xl text-center">
      <p className="font-mono text-xs uppercase tracking-[0.18em] text-fg/45">{kicker}</p>
      <h2 className="mt-5 text-balance text-4xl font-semibold leading-[1.02] tracking-[-0.04em] md:text-[3.4rem]">{title}</h2>
      {lede ? <p className="mx-auto mt-5 max-w-xl text-pretty text-lg leading-relaxed text-fg/55">{lede}</p> : null}
    </div>
  )
}

/** The dim second half of a headline. */
export function Dim({ children }: { children: ReactNode }) {
  return <span className="text-fg/40">{children}</span>
}

export function Card({ className = '', children }: { className?: string; children: ReactNode }) {
  return (
    <div className={`relative overflow-hidden rounded-2xl border border-fg/10 bg-fg/[0.025] ${className}`}>{children}</div>
  )
}
