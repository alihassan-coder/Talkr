export function SpeedControl({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return (
    <label className="inline-flex h-9 items-center gap-3 rounded-full border border-fg/12 px-3.5 text-[13px] transition-colors hover:border-fg/20 focus-within:border-fg/30">
      <span className="text-fg/45">Speed</span>
      <input
        type="range"
        min={0.5}
        max={2}
        step={0.05}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={() => onChange(1)}
        aria-valuetext={`${value.toFixed(2)} times`}
        className="w-24 cursor-pointer"
      />
      <span className="w-10 font-mono text-[12px] tabular-nums text-fg">{value.toFixed(2)}×</span>
    </label>
  )
}
