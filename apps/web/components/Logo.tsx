import Link from 'next/link'

/** Lines of text turning into a waveform: words in, voice out. */
export function LogoMark({ className = 'size-6' }: { className?: string }) {
  return (
    <svg viewBox="5 5 54 54" className={className} aria-hidden="true" focusable="false">
      <g className="fill-fg">
        <rect x="7.25" y="21" width="22" height="5.5" rx="2.75" />
        <rect x="7.25" y="29.25" width="15" height="5.5" rx="2.75" />
        <rect x="7.25" y="37.5" width="19" height="5.5" rx="2.75" />
        <rect x="34.25" y="22" width="5.5" height="20" rx="2.75" />
        <rect x="42.75" y="13" width="5.5" height="38" rx="2.75" />
        <rect x="51.25" y="24.5" width="5.5" height="15" rx="2.75" />
      </g>
    </svg>
  )
}

export function Logo() {
  return (
    <Link href="/" className="group inline-flex items-center gap-2" aria-label="Talkr, home">
      <LogoMark className="size-6 transition-transform duration-500 ease-out-quint group-hover:scale-110" />
      <span className="text-[17px] font-semibold tracking-[-0.03em]">Talkr</span>
    </Link>
  )
}
