import { Check } from 'lucide-react'
import { LogoMark } from '@/components/Logo'
import { Container, Dim, SectionIntro } from '@/components/ui'

const rows: { label: string; cloud: string; talkr: string; positive: boolean }[] = [
  { label: 'Where your audio goes', cloud: 'Uploaded to their servers', talkr: 'Stays on your disk', positive: true },
  { label: 'Works offline', cloud: 'No', talkr: 'Yes, after the model downloads', positive: true },
  { label: 'Account', cloud: 'Required', talkr: 'None', positive: true },
  { label: 'Price', cloud: 'Per minute or monthly', talkr: 'Free, MIT licensed', positive: true },
  { label: 'Speed', cloud: 'Depends on your connection', talkr: 'Depends on your GPU', positive: false },
  { label: 'Can you read the code?', cloud: 'Usually not', talkr: 'Every line', positive: true },
]

const headCell = 'border-b border-fg/10 px-5 py-4 text-left font-normal'
const monoHead = 'font-mono text-xs uppercase tracking-[0.14em] text-fg/45'

export function Compare() {
  return (
    <section id="compare" className="py-24 md:py-32">
      <Container>
        <SectionIntro
          kicker="Compared"
          title={
            <>
              Same job. <Dim>Different address.</Dim>
            </>
          }
          lede={
            <>
              Cloud speech tools work well. They also need your audio on someone else&apos;s computer. Here is the trade,
              laid out plainly.
            </>
          }
        />

        <div className="reveal mx-auto mt-16 max-w-4xl overflow-x-auto rounded-2xl border border-fg/10">
          <table className="w-full min-w-[560px] border-collapse">
            <caption className="sr-only">Talkr compared with a typical cloud speech service</caption>
            <thead>
              <tr>
                <th scope="col" className={`${headCell} ${monoHead}`}>
                  <span className="sr-only">Feature</span>
                </th>
                <th scope="col" className={`${headCell} ${monoHead}`}>
                  Typical cloud service
                </th>
                <th scope="col" className={`${headCell} bg-fg/[0.04]`}>
                  <span className="inline-flex items-center gap-2 font-medium text-fg">
                    <LogoMark className="size-4" />
                    Talkr
                  </span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.label} className="border-b border-fg/[0.08] last:border-0">
                  <th scope="row" className="px-5 py-4 text-left text-[15px] font-medium text-fg/80">
                    {r.label}
                  </th>
                  <td className="px-5 py-4 text-[15px] text-fg/50">{r.cloud}</td>
                  <td className="bg-fg/[0.04] px-5 py-4 text-[15px] text-fg">
                    {r.positive ? (
                      <Check aria-hidden="true" strokeWidth={1.75} className="mr-2 inline size-3.5 align-[-0.1em] text-fg" />
                    ) : null}
                    {r.talkr}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <p className="mt-6 text-center text-sm text-fg/45">
          Cloud tools still win on some things, like speaker labels and very long files on weak hardware. Talkr is for
          when the audio shouldn&apos;t leave the room.
        </p>
      </Container>
    </section>
  )
}
