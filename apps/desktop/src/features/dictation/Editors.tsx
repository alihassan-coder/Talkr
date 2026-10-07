import { useState } from 'react'
import type { FormEvent } from 'react'
import { ArrowRight, Plus, X } from 'lucide-react'
import type { AppMethod, AppRule, Replacement } from '@/lib/types'
import { Badge, Button } from '@/components/ui'
import { Select } from '@/components/Select'
import { toExe } from '@/features/dictation/utils'

const input =
  'h-8 min-w-0 rounded-lg border border-line bg-bg px-2.5 text-[13px] text-fg placeholder:text-subtle transition-colors focus:border-line-strong focus:outline-none'

/** Words whisper should spell your way: names, products, jargon. Enter or comma adds one. */
export function VocabularyEditor({ words, onChange }: { words: string[]; onChange: (words: string[]) => void }) {
  const [draft, setDraft] = useState('')
  const add = () => {
    const fresh = draft
      .split(',')
      .map((w) => w.trim())
      .filter((w) => w && w.length <= 100 && !words.some((x) => x.toLowerCase() === w.toLowerCase()))
    if (fresh.length) onChange([...words, ...fresh].slice(0, 200))
    setDraft('')
  }
  return (
    <div className="space-y-3 px-5 py-4">
      <div className="flex flex-wrap gap-1.5">
        {words.length === 0 ? <p className="text-[12.5px] text-subtle">No words yet.</p> : null}
        {words.map((w) => (
          <span key={w} className="inline-flex h-7 items-center gap-1 rounded-full border border-line bg-bg pl-2.5 pr-1 text-[12.5px]">
            {w}
            <button
              type="button"
              aria-label={`Remove ${w}`}
              className="grid size-5 place-items-center rounded-full text-subtle hover:bg-fg/10 hover:text-fg"
              onClick={() => onChange(words.filter((x) => x !== w))}
            >
              <X className="size-3" strokeWidth={2.25} />
            </button>
          </span>
        ))}
      </div>
      <form
        className="flex gap-2"
        onSubmit={(e: FormEvent) => {
          e.preventDefault()
          add()
        }}
      >
        <input
          className={`${input} flex-1`}
          value={draft}
          maxLength={400}
          placeholder="Add names or terms, e.g. Talkr, Kubernetes, Aisha"
          aria-label="New vocabulary words"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === ',') {
              e.preventDefault()
              add()
            }
          }}
        />
        <Button size="sm" type="submit" disabled={!draft.trim()} icon={<Plus className="size-3.5" strokeWidth={2} />}>
          Add
        </Button>
      </form>
    </div>
  )
}

/** "When I say X, type Y." */
export function ReplacementsEditor({ items, onChange }: { items: Replacement[]; onChange: (items: Replacement[]) => void }) {
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const add = () => {
    const f = from.trim()
    if (!f) return
    onChange([...items.filter((r) => r.from.toLowerCase() !== f.toLowerCase()), { from: f, to: to.trim() }].slice(0, 200))
    setFrom('')
    setTo('')
  }
  return (
    <div className="space-y-3 px-5 py-4">
      {items.length ? (
        <ul className="space-y-1.5" aria-label="Replacements">
          {items.map((r) => (
            <li key={r.from} className="flex items-center gap-2 text-[13px]">
              <span className="min-w-0 flex-1 truncate rounded-lg bg-fg/[0.04] px-2.5 py-1">{r.from}</span>
              <ArrowRight className="size-3.5 shrink-0 text-subtle" strokeWidth={2} aria-label="becomes" />
              <span className="min-w-0 flex-1 truncate rounded-lg bg-fg/[0.04] px-2.5 py-1 font-medium">
                {r.to || <em className="font-normal text-subtle">removed</em>}
              </span>
              <button
                type="button"
                aria-label={`Remove replacement for ${r.from}`}
                className="grid size-7 shrink-0 place-items-center rounded-lg text-subtle hover:bg-fg/[0.06] hover:text-fg"
                onClick={() => onChange(items.filter((x) => x !== r))}
              >
                <X className="size-3.5" strokeWidth={2} />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-[12.5px] text-subtle">No replacements yet.</p>
      )}
      <form
        className="flex items-center gap-2"
        onSubmit={(e: FormEvent) => {
          e.preventDefault()
          add()
        }}
      >
        <input
          className={`${input} flex-1`}
          value={from}
          maxLength={100}
          placeholder="When I say…"
          aria-label="Words to replace"
          onChange={(e) => setFrom(e.target.value)}
        />
        <ArrowRight className="size-3.5 shrink-0 text-subtle" strokeWidth={2} aria-hidden="true" />
        <input
          className={`${input} flex-1`}
          value={to}
          maxLength={500}
          placeholder="…type this"
          aria-label="Replacement"
          onChange={(e) => setTo(e.target.value)}
        />
        <Button size="sm" type="submit" disabled={!from.trim()} icon={<Plus className="size-3.5" strokeWidth={2} />}>
          Add
        </Button>
      </form>
    </div>
  )
}

const methodOptions: { value: AppMethod; label: string }[] = [
  { value: 'auto', label: 'Automatic' },
  { value: 'paste', label: 'Paste' },
  { value: 'type', label: 'Type keys' },
  { value: 'off', label: 'Off (copy only)' },
]

/** How text goes into particular apps. Talkr adds rules itself when pasting fails somewhere. */
export function AppRulesEditor({ rules, onChange }: { rules: AppRule[]; onChange: (rules: AppRule[]) => void }) {
  const [app, setApp] = useState('')
  const add = () => {
    const exe = toExe(app)
    if (!exe || exe === '.exe') return
    onChange([...rules.filter((r) => r.app !== exe), { app: exe, method: 'type', learned: false }])
    setApp('')
  }
  return (
    <div className="space-y-3 px-5 py-4">
      {rules.length ? (
        <ul className="divide-y divide-line rounded-xl border border-line" aria-label="App rules">
          {rules.map((r) => (
            <li key={r.app} className="flex items-center gap-3 px-3 py-2">
              <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]">{r.app}</span>
              {r.learned ? <Badge>Learned</Badge> : null}
              <Select
                label=""
                aria-label={`Method for ${r.app}`}
                value={r.method}
                options={methodOptions}
                onChange={(method) =>
                  onChange(rules.map((x) => (x.app === r.app ? { ...x, method: method as AppMethod, learned: false } : x)))
                }
              />
              <button
                type="button"
                aria-label={`Remove rule for ${r.app}`}
                className="grid size-7 shrink-0 place-items-center rounded-lg text-subtle hover:bg-fg/[0.06] hover:text-fg"
                onClick={() => onChange(rules.filter((x) => x.app !== r.app))}
              >
                <X className="size-3.5" strokeWidth={2} />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-[12.5px] text-subtle">
          No rules. Talkr picks the right way for each app, and adds a rule here by itself if an app refuses pasted
          text.
        </p>
      )}
      <form
        className="flex gap-2"
        onSubmit={(e: FormEvent) => {
          e.preventDefault()
          add()
        }}
      >
        <input
          className={`${input} flex-1 font-mono`}
          value={app}
          maxLength={120}
          placeholder="App, e.g. mstsc.exe"
          aria-label="App to add a rule for"
          onChange={(e) => setApp(e.target.value)}
        />
        <Button size="sm" type="submit" disabled={!app.trim()} icon={<Plus className="size-3.5" strokeWidth={2} />}>
          Add rule
        </Button>
      </form>
    </div>
  )
}
