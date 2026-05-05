import type * as React from 'react'
import type { StyleProp } from './css'

const compilerSetupMessage =
  'Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.'

type HtmlProps<TagName extends keyof React.JSX.IntrinsicElements> = Omit<
  React.JSX.IntrinsicElements[TagName],
  'style'
> & {
  style?: StyleProp
}

type HtmlElements = {
  [TagName in keyof React.JSX.IntrinsicElements]: (
    props: HtmlProps<TagName>,
  ) => React.ReactElement | null
}

const html = new Proxy(
  {},
  {
    get(_target, tagName) {
      throw new Error(
        `[nanocss] html.${String(tagName)} is a compile-time API. ${compilerSetupMessage}`,
      )
    },
  },
) as HtmlElements

export { html }
