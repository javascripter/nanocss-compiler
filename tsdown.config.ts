import { defineConfig } from 'tsdown'

export default defineConfig((options) => ({
  entry: {
    index: 'src/index.ts',
    'postcss-plugin': 'src/postcss-plugin/index.ts',
    transform: 'src/transform.ts',
  },
  format: ['cjs', 'esm'],
  dts: true,
  clean: true,
  sourcemap: true,
  minify: false,
  splitting: true,
  cjsDefault: true,
}))
