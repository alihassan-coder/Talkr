/**
 * Small colour toolkit for the custom theme: hex parsing, sRGB <-> OKLCH and
 * WCAG contrast. OKLCH keeps lightness perceptual, so a palette derived from
 * any accent lands at the same visual weight as the built-in ones.
 */

export type Oklch = { l: number; c: number; h: number }

/** `#rgb` / `#rrggbb`, with or without the `#`, to lowercase `#rrggbb`; null if it is not a colour. */
export function normalizeHex(value: string): string | null {
  const raw = value.trim().replace(/^#/, '').toLowerCase()
  if (/^[0-9a-f]{6}$/.test(raw)) return `#${raw}`
  if (/^[0-9a-f]{3}$/.test(raw)) return `#${[...raw].map((ch) => ch + ch).join('')}`
  return null
}

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)
const toGamma = (v: number) => (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055)

/** Channels 0..1 of a `#rrggbb` colour. */
const rgbOf = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16)
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}

const hexOf = ([r, g, b]: [number, number, number]) =>
  `#${[r, g, b]
    .map((v) =>
      Math.round(Math.min(1, Math.max(0, v)) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`

export function hexToOklch(hex: string): Oklch {
  const [r, g, b] = rgbOf(hex).map(toLinear) as [number, number, number]
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
  const h = (Math.atan2(B, A) * 180) / Math.PI
  return { l: L, c: Math.hypot(A, B), h: h < 0 ? h + 360 : h }
}

/** Linear sRGB of an OKLCH colour, possibly out of gamut. */
function oklchToLinear({ l: L, c, h }: Oklch): [number, number, number] {
  const A = c * Math.cos((h * Math.PI) / 180)
  const B = c * Math.sin((h * Math.PI) / 180)
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ]
}

const inGamut = (rgb: number[]) => rgb.every((v) => v >= -0.0001 && v <= 1.0001)

/** OKLCH to hex. Out-of-gamut colours lose chroma (not lightness or hue) until they fit. */
export function oklchToHex(color: Oklch): string {
  const l = Math.min(1, Math.max(0, color.l))
  let rgb = oklchToLinear({ ...color, l })
  if (!inGamut(rgb)) {
    let lo = 0
    let hi = color.c
    for (let i = 0; i < 20; i++) {
      const mid = (lo + hi) / 2
      if (inGamut(oklchToLinear({ ...color, l, c: mid }))) lo = mid
      else hi = mid
    }
    rgb = oklchToLinear({ ...color, l, c: lo })
  }
  return hexOf(rgb.map(toGamma) as [number, number, number])
}

const luminance = (hex: string) => {
  const [r, g, b] = rgbOf(hex).map(toLinear) as [number, number, number]
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** WCAG 2 contrast ratio, 1..21. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number]
  return (hi + 0.05) / (lo + 0.05)
}
