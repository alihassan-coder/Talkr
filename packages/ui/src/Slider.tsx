import { forwardRef, InputHTMLAttributes } from 'react'

interface SliderProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string
  min?: number
  max?: number
  step?: number
  showValue?: boolean
  valueFormat?: (value: number) => string
}

export const Slider = forwardRef<HTMLInputElement, SliderProps>(
  ({ className = '', label, min = 0, max = 1, step = 0.01, showValue = false, valueFormat, id, ...props }, ref) => {
    const sliderId = id || label?.toLowerCase().replace(/\s+/g, '-')
    const value = Number(props.value) || min
    const percentage = ((value - min) / (max - min)) * 100

    return (
      <div className="w-full">
        <div className="flex items-baseline justify-between mb-1.5">
          {label && (
            <label htmlFor={sliderId} className="block text-sm font-medium text-text-primary">
              {label}
            </label>
          )}
          {showValue && (
            <span className="text-sm text-text-secondary font-mono tabular-nums">
              {valueFormat ? valueFormat(value) : value.toFixed(step < 1 ? 2 : 0)}
            </span>
          )}
        </div>
        <div className="relative">
          <input
            ref={ref}
            id={sliderId}
            type="range"
            min={min}
            max={max}
            step={step}
            className={`
              w-full h-2 appearance-none bg-border rounded-full
              cursor-pointer transition-colors duration-fast
              focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2
              disabled:opacity-50 disabled:cursor-not-allowed
              ${className}
            `}
            style={{
              background: `linear-gradient(to right, var(--color-accent) ${percentage}%, var(--color-border) ${percentage}%)`,
            } as React.CSSProperties}
            {...props}
          />
        </div>
      </div>
    )
  }
)

Slider.displayName = 'Slider'