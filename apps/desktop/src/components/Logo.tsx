/** A speech bubble with three audio bars knocked out of it. Same mark as the website. */
export function LogoMark({ className = 'size-6' }: { className?: string }) {
  return (
    <svg viewBox="0 0 28 28" className={className} aria-hidden="true" focusable="false">
      <path
        d="M14 2C7.37 2 2 7.37 2 14c0 2.4.7 4.63 1.9 6.5L2.5 25.5l5-1.4A11.94 11.94 0 0 0 14 26c6.63 0 12-5.37 12-12S20.63 2 14 2Z"
        className="fill-fg"
      />
      <g className="fill-bg">
        <rect x="8.5" y="11.5" width="2" height="5" rx="1" />
        <rect x="13" y="8.5" width="2" height="11" rx="1" />
        <rect x="17.5" y="11" width="2" height="6" rx="1" />
      </g>
    </svg>
  )
}
