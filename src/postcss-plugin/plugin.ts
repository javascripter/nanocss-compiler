import postcss from 'postcss'
import type { Helpers, Root } from 'postcss'
import { createNanoCssBuilder } from './builder'
import type { NanoCssPostCssPluginOptions } from './types'

function createNanoCssPostCssPlugin(
  options: NanoCssPostCssPluginOptions = {},
) {
  const pluginName = 'postcss-nanocss'
  const builder = createNanoCssBuilder()
  let shouldSkipTransformError = false

  return {
    postcssPlugin: pluginName,
    async Once(root: Root, { result }: Helpers) {
      builder.configure(options)

      const atRule = builder.findAtRule(root)
      if (!atRule) {
        return
      }

      for (const dependency of builder.getDependencies()) {
        result.messages.push({
          plugin: pluginName,
          parent: result.opts.from,
          ...dependency,
        })
      }

      const css = await builder.build({ shouldSkipTransformError })
      const parsed = postcss.parse(css, { from: result.opts.from })
      atRule.replaceWith(...parsed.nodes)

      if (!shouldSkipTransformError) {
        shouldSkipTransformError = true
      }
    },
  }
}

createNanoCssPostCssPlugin.postcss = true

export { createNanoCssPostCssPlugin }
