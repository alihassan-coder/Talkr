import type { Config } from 'tailwindcss'
import preset from '@talkr/ui/tailwind-preset'

export default {
  content: [
    './index.html',
    './src/**/*.{ts,tsx}',
    '../../packages/ui/src/**/*.{ts,tsx}'
  ],
  presets: [preset],
  theme: {
    extend: {}
  }
} satisfies Config