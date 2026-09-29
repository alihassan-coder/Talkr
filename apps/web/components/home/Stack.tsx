import { Container } from '@/components/ui'

const items = ['whisper.cpp', 'sherpa-onnx', 'Kokoro', 'Piper', 'Metal', 'CUDA', 'Vulkan', 'Tauri']

export function Stack() {
  return (
    <section aria-label="Built on" className="py-20 md:py-28">
      <Container>
        <p className="text-center text-sm text-fg/40">Built on open models and runtimes you can inspect yourself</p>
        <ul className="mx-auto mt-8 flex max-w-4xl flex-wrap items-center justify-center gap-x-10 gap-y-4">
          {items.map((item) => (
            <li key={item} className="font-mono text-[15px] tracking-[-0.01em] text-fg/35 transition-colors hover:text-fg/80">
              {item}
            </li>
          ))}
        </ul>
      </Container>
    </section>
  )
}
