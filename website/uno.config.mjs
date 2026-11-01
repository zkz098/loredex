import {
  defineConfig,
  presetUno,
  presetAttributify,
  presetTypography,
} from 'unocss';

export default defineConfig({
  presets: [
    presetUno(),
    presetAttributify(),
    presetTypography(),
  ],
  theme: {
    colors: {
      brand: {
        50: '#f0fdfa',
        100: '#ccfbf1',
        200: '#99f6e4',
        300: '#5eead4',
        400: '#2dd4bf',
        500: '#14b8a6',
        600: '#0d9488',
        700: '#0f766e',
        800: '#115e59',
        900: '#134e4a',
        950: '#042f2e',
      },
      accent: {
        50: '#eef2ff',
        100: '#e0e7ff',
        200: '#c7d2fe',
        300: '#a5b4fc',
        400: '#818cf8',
        500: '#6366f1',
        600: '#4f46e5',
        700: '#4338ca',
        800: '#3730a3',
        900: '#312e81',
      },
      dark: {
        bg: '#0a0d14',
        card: '#111726',
        border: '#1e293b',
        muted: '#94a3b8',
      }
    },
    fontFamily: {
      sans: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
      mono: 'ui-monospace, SFMono-Regular, "Cascadia Code", "Fira Code", Menlo, Monaco, Consolas, monospace',
    },
  },
  shortcuts: {
    'btn-primary': 'inline-flex items-center justify-center gap-2 px-5 py-2.5 rounded-lg font-medium text-sm text-white bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 shadow-lg shadow-emerald-500/20 transition-all duration-200 transform hover:-translate-y-0.5 active:translate-y-0 cursor-pointer border border-emerald-400/30',
    'btn-secondary': 'inline-flex items-center justify-center gap-2 px-5 py-2.5 rounded-lg font-medium text-sm text-slate-200 bg-slate-800/80 hover:bg-slate-700/80 hover:text-white border border-slate-700/60 shadow-sm transition-all duration-200 cursor-pointer backdrop-blur-sm',
    'badge-pill': 'inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium backdrop-blur-md',
    'card-glow': 'relative rounded-2xl bg-slate-900/60 border border-slate-800/80 hover:border-slate-700/80 backdrop-blur-xl transition-all duration-300 hover:shadow-xl hover:shadow-emerald-500/5',
    'glass-panel': 'bg-slate-900/70 border border-slate-800/80 backdrop-blur-xl',
  },
});
