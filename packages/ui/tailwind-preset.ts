import type { Config } from 'tailwindcss'

const preset: Config = {
  theme: {
    extend: {
      colors: {
        bg: {
          primary: '#FFFFFF',
          canvas: '#F7F7FA',
          card: '#FFFFFF',
        },
        text: {
          primary: '#0F1115',
          secondary: '#5B6070',
          muted: '#9AA0AE',
        },
        accent: {
          DEFAULT: '#5B5BF7',
          hover: '#4A4AE6',
          soft: '#EEEEFF',
        },
        success: {
          DEFAULT: '#16C79A',
          soft: '#E6FAF5',
        },
        warning: {
          DEFAULT: '#F5A524',
          soft: '#FFF7E6',
        },
        danger: {
          DEFAULT: '#EF4444',
          soft: '#FEF2F2',
        },
        border: '#ECECF1',
      },
      borderRadius: {
        card: '24px',
        input: '14px',
        pill: '9999px',
      },
      boxShadow: {
        soft: '0 1px 2px rgba(16,17,21,0.04), 0 8px 24px rgba(16,17,21,0.06)',
        card: '0 1px 2px rgba(16,17,21,0.04), 0 4px 12px rgba(16,17,21,0.05)',
      },
      fontFamily: {
        sans: ['Inter Variable', 'system-ui', 'sans-serif'],
      },
      transitionDuration: {
        'fast': '150ms',
        'normal': '200ms',
      },
      transitionTimingFunction: {
        'ease-out': 'ease-out',
      },
    },
  },
}

export default preset