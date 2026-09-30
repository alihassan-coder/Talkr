import { Card, Container, Dim, SectionIntro } from '@/components/ui'
import {
  type CatalogModel,
  describeLanguages,
  formatBytes,
  isCompressed,
  isRecommended,
  models,
  shortLicense,
  splitName,
  sttModels,
  ttsModels,
} from '@/lib/models'

function ModelList({ title, meta, items }: { title: string; meta: string; items: CatalogModel[] }) {
  return (
    <Card className="p-2">
      <div className="flex items-baseline justify-between px-4 pb-3 pt-4">
        <h3 className="text-lg font-medium tracking-[-0.02em]">{title}</h3>
        <span className="font-mono text-[11px] text-fg/40">{meta}</span>
      </div>
      <ul>
        {items.map((model) => {
          const { base, variant } = splitName(model.name)
          return (
            <li
              key={model.id}
              className="flex items-center gap-4 rounded-xl px-4 py-3.5 transition-colors hover:bg-fg/[0.04]"
            >
              <div className="min-w-0 flex-1">
                <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[15px]">
                  <span className="font-medium">{base}</span>
                  {variant ? <span className="text-fg/45">{variant}</span> : null}
                  {isRecommended(model) ? (
                    <span className="rounded-full bg-fg px-2 py-px text-[10px] font-medium text-bg">Recommended</span>
                  ) : null}
                  {isCompressed(model) ? (
                    <span className="rounded-full border border-fg/15 px-2 py-px text-[10px] font-medium text-fg/55">
                      Compressed · less memory
                    </span>
                  ) : null}
                </p>
                <p className="mt-1 font-mono text-[11px] text-fg/40" title={model.license}>
                  {describeLanguages(model.languages)} · {shortLicense(model.license)}
                </p>
              </div>
              <span className="shrink-0 font-mono text-[13px] tabular-nums text-fg/70">{formatBytes(model.sizeBytes)}</span>
            </li>
          )
        })}
      </ul>
    </Card>
  )
}

export function Models() {
  return (
    <section id="models" className="py-24 md:py-32">
      <Container>
        <SectionIntro
          kicker="Model library"
          title={
            <>
              {models.length} open models. <Dim>One click each.</Dim>
            </>
          }
          lede="Download from inside the app, check the license first, delete whenever you like. Already have a GGML or ONNX file? Import it."
        />
        <div className="reveal mt-16 grid items-start gap-4 lg:grid-cols-2">
          <ModelList title="Speech to text" meta="Whisper · GGML" items={sttModels} />
          <ModelList title="Text to speech" meta="Kokoro, Piper · ONNX" items={ttsModels} />
        </div>
      </Container>
    </section>
  )
}
