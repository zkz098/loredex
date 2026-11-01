import { defineConfig } from 'astro/config';
import UnoCSS from '@unocss/astro';

export default defineConfig({
  i18n: {
    defaultLocale: 'zh',
    locales: ['zh', 'en'],
    routing: {
      prefixDefaultLocale: false,
    },
  },
  integrations: [
    UnoCSS({
      injectReset: false,
    }),
  ],
});
