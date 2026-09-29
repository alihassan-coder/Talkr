import { Info } from 'lucide-react'

/** Shown when the UI runs in a plain browser, without the Tauri backend. */
export function PreviewNotice({ children }: { children: string }) {
  return (
    <div className="flex items-center gap-3 rounded-xl border border-fg/10 px-4 py-3 text-[13px] text-fg/55">
      <Info className="size-4 shrink-0 text-fg/45" strokeWidth={1.75} />
      <p>{children}</p>
    </div>
  )
}
