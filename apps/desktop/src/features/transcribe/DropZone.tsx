import { useEffect, useEffectEvent, useState } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { Upload } from 'lucide-react'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { isTauri } from '@/lib/api'
import { toast, toastError } from '@/stores/toast'
import { AUDIO_EXTENSIONS, isAudioPath } from './utils'

const unsupported = 'That file type is not supported. Try MP3, WAV, FLAC, OGG or M4A.'

export function DropZone({ onFile, disabled = false }: { onFile: (path: string) => void; disabled?: boolean }) {
  const [over, setOver] = useState(false)

  const handleDrop = useEffectEvent((paths: string[]) => {
    if (disabled) return
    const path = paths.find(isAudioPath)
    if (path) onFile(path)
    else toast(unsupported)
  })

  useEffect(() => {
    if (!isTauri()) return
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === 'enter' || payload.type === 'over') setOver(true)
      else if (payload.type === 'leave') setOver(false)
      else {
        setOver(false)
        handleDrop(payload.paths)
      }
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [])

  const choose = async () => {
    if (!isTauri()) {
      toast('Opening files works in the Talkr desktop app.')
      return
    }
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Audio', extensions: AUDIO_EXTENSIONS }],
      })
      if (typeof picked === 'string') onFile(picked)
    } catch (err) {
      toastError(err)
    }
  }

  return (
    <div
      className={cx(
        'flex flex-col items-center rounded-2xl border border-dashed px-8 py-14 text-center transition-colors duration-300 ease-out-quint',
        over && !disabled ? 'border-fg/40 bg-fg/[0.05]' : 'border-fg/15 bg-fg/[0.015]',
      )}
    >
      <span
        className={cx(
          'grid size-12 place-items-center rounded-full border transition-[transform,border-color,color] duration-300 ease-out-quint',
          over ? '-translate-y-1 border-fg/30 text-fg' : 'border-fg/10 text-fg/55',
        )}
      >
        <Upload className="size-5" strokeWidth={1.75} />
      </span>
      <p className="mt-5 text-[15px] font-medium tracking-[-0.01em]">{over ? 'Release to transcribe' : 'Drop an audio file'}</p>
      <p className="mt-1.5 font-mono text-[11px] tracking-wide text-fg/40">MP3, WAV, FLAC, OGG, M4A</p>
      <Button className="mt-6" onClick={() => void choose()} disabled={disabled}>
        Choose file
      </Button>
    </div>
  )
}
