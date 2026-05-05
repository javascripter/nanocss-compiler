import { execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { transformSync as transformSwcSync } from '@swc/core'
import { beforeAll, describe, expect, it } from 'vitest'
import { transformSync as transformNanoCssSync } from '../native'

type TransformOptions = {
  debug?: boolean
  importSources?: string[]
  filename?: string
  inputSourceMap?: string | Record<string, unknown>
  htmlDefaults?: Record<string, Record<string, string | number | boolean | null>>
  env?: Record<string, unknown>
}

const require = createRequire(import.meta.url)
const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..',
)

function transformWithMetadata(source: string, options?: TransformOptions) {
  const { filename = 'test.tsx', inputSourceMap, ...pluginOptions } = options ?? {}
  const debug = pluginOptions.debug ?? true
  const swcInputSourceMap =
    typeof inputSourceMap === 'string'
      ? inputSourceMap
      : inputSourceMap
        ? JSON.stringify(inputSourceMap)
        : undefined
  let code: string
  try {
    code = transformSwcSync(source, {
      filename,
      inputSourceMap: swcInputSourceMap,
      jsc: {
        target: 'es2022',
        parser: {
          syntax: 'typescript',
          tsx: true,
        },
        transform: {
          react: {
            runtime: 'preserve',
          },
        },
        experimental: {
          plugins: [
            [
              require.resolve('nanocss-compiler/swc'),
              {
                debug,
                inputSourceMap,
                ...pluginOptions,
              },
            ],
          ],
        },
      },
      module: {
        type: 'es6',
      },
    }).code
  } catch (error) {
    try {
      transformNanoCssSync(source, {
        filename,
        debug,
        importSources: pluginOptions.importSources,
        inputSourceMap: swcInputSourceMap,
        htmlDefaults: pluginOptions.htmlDefaults,
        env: pluginOptions.env,
      })
    } catch (nativeError) {
      throw nativeError
    }
    throw error
  }

  const nativeResult = transformNanoCssSync(source, {
    filename,
    debug,
    importSources: pluginOptions.importSources,
    inputSourceMap: swcInputSourceMap,
    htmlDefaults: pluginOptions.htmlDefaults,
    env: pluginOptions.env,
  })

  return {
    code,
    metadata: {
      nanocss: {
        styleSheet: nativeResult.metadata.nanocss.styleSheet,
      },
    },
  }
}

function transform(source: string, options?: TransformOptions) {
  return transformWithMetadata(source, options).code
}

describe('nanocss swc plugin', () => {
  beforeAll(() => {
    execFileSync('bun', ['run', 'build:swc'], {
      cwd: repoRoot,
      stdio: 'pipe',
    })
    execFileSync('bun', ['run', 'build:node'], {
      cwd: repoRoot,
      stdio: 'pipe',
    })
  }, 120_000)

  it('compiles local static props calls to hoisted style objects', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const marginRight = 8
        const styles = css.create({
          a: {
            marginLeft: 0,
            color: 'red',
          },
          b: {
            marginRight,
            color: 'blue',
          },
        })

        function Comp() {
          return <div {...css.props(styles.a, styles.b)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesA = {
          marginLeft: 0,
          color: 'red'
      };
      const _stylesB = {
          marginRight: 8,
          color: 'blue'
      };
      function Comp() {
          return <div style={{
              ..._stylesA,
              ..._stylesB
          }}/>;
      }
      "
    `)
  })

  it('dedupes repeated props combinations', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          a: {
            opacity: 1,
          },
          b: {
            display: 'flex',
          },
        })

        function Comp() {
          return (
            <>
              <div {...css.props(styles.a, styles.b)} />
              <span {...css.props(styles.a, styles.b)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesAB = {
          opacity: 1,
          display: 'flex'
      };
      function Comp() {
          return <>
                    <div style={_stylesAB}/>
                    <span style={_stylesAB}/>
                  </>;
      }
      "
    `)
  })

  it('does not merge overlapping static props combinations', () => {
    const output = transform(`
      import { css } from 'nanocss-compiler'

      const styles = css.create({
        a: {
          opacity: 1,
        },
        b: {
          display: 'flex',
        },
        c: {
          color: 'red',
        },
      })

      function Comp() {
        return (
          <>
            <div {...css.props(styles.a, styles.b, styles.c)} />
            <span {...css.props(styles.b, styles.c)} />
            <button {...css.props(styles.a, styles.b)} />
          </>
        )
      }
    `)

    expect(output).toContain('..._stylesA')
    expect(output).toContain('..._stylesB')
    expect(output).toContain('..._stylesC')
    expect(output).not.toContain('_stylesAB')
    expect(output).not.toContain('_stylesBC')
    expect(output).not.toContain('_stylesABC')
  })

  it('does not merge repeated static props combinations with non-static member usage', () => {
    const output = transform(`
      import { css } from 'nanocss-compiler'

      const styles = css.create({
        a: {
          opacity: 1,
        },
        b: {
          display: 'flex',
        },
      })

      function Comp() {
        return (
          <>
            <div {...css.props(styles.a, styles.b)} />
            <span {...css.props(styles.a, styles.b)} />
            <button {...css.props(isActive && styles.a)} />
          </>
        )
      }
    `)

    expect(output).toContain('..._stylesA')
    expect(output).toContain('..._stylesB')
    expect(output).not.toContain('_stylesAB')
  })

  it('does not merge repeated static props combinations with dynamic group member usage', () => {
    const output = transform(`
      import { css } from 'nanocss-compiler'

      const styles = css.create({
        a: {
          opacity: 1,
        },
        b: {
          display: 'flex',
        },
      })

      function Comp() {
        return (
          <>
            <div {...css.props(styles.a, styles.b)} />
            <span {...css.props(styles.a, styles.b)} />
            <button {...css.props(styles[getStyleName()])} />
          </>
        )
      }
    `)

    expect(output).toContain('..._stylesA')
    expect(output).toContain('..._stylesB')
    expect(output).not.toContain('_stylesAB')
  })

  it('dedupes adjacent repeated static style refs', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: getColor(),
          },
        })

        function Comp() {
          return <div {...css.props(styles.root, styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: getColor()
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('uses style group names in debug helper names', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const fooStyles = css.create({
          bar: {
            opacity: 1,
          },
          baz: {
            color: 'red',
          },
        })

        function Comp() {
          return <div {...css.props(fooStyles.bar, fooStyles.baz)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _fooStylesBar = {
          opacity: 1
      };
      const _fooStylesBaz = {
          color: 'red'
      };
      function Comp() {
          return <div style={{
              ..._fooStylesBar,
              ..._fooStylesBaz
          }}/>;
      }
      "
    `)
  })

  it('separates mixed style group names in debug helper names', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const fooStyles = css.create({
          a: {
            opacity: 1,
          },
        })
        const barStyles = css.create({
          b: {
            color: 'red',
          },
        })

        function Comp() {
          return <div {...css.props(fooStyles.a, barStyles.b)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _fooStylesA = {
          opacity: 1
      };
      const _barStylesB = {
          color: 'red'
      };
      function Comp() {
          return <div style={{
              ..._fooStylesA,
              ..._barStylesB
          }}/>;
      }
      "
    `)
  })

  it('precomputes hook values and records inferred hooks in css metadata', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':hover': 'red',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('inlines local literal constants in computed style and hook keys', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const property = 'color'
        const hover = ':hover'
        const styles = css.create({
          root: {
            [property]: {
              default: 'black',
              [hover]: 'red',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_hover-mbscpo-0: initial;
        --_hover-mbscpo-1: ;
      }
      *:hover {
        --_hover-mbscpo-0: ;
        --_hover-mbscpo-1: initial;
      }
      "
    `)
  })

  it('compiles static keyframes and records css metadata', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

      const fadeIn = css.keyframes({
        '0%': {
          opacity: 0,
        },
        '100%': {
          opacity: 1,
        },
      })

      const styles = css.create({
        root: {
          animationName: fadeIn,
          animationDuration: '1s',
        },
      })

      function Comp() {
        return <div {...css.props(styles.root)} />
      }
    `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const fadeIn = "__nanocss_keyframes-1ii5yk";
      const _stylesRoot = {
          animationName: fadeIn,
          animationDuration: '1s'
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "@keyframes __nanocss_keyframes-1ii5yk {
        0% {
          opacity: 0;
        }
        100% {
          opacity: 1;
        }
      }"
    `)
  })

  it('dedupes identical generated css metadata blocks', () => {
    const result = transformWithMetadata(`
      import { css } from 'nanocss-compiler'

      const fadeIn = css.keyframes({
        from: { opacity: 0 },
        to: { opacity: 1 },
      })

      const alsoFadeIn = css.keyframes({
        from: { opacity: 0 },
        to: { opacity: 1 },
      })

      const styles = css.create({
        root: {
          animationName: fadeIn,
        },
        other: {
          animationName: alsoFadeIn,
        },
      })

      function Comp() {
        return <div {...css.props(styles.root, styles.other)} />
      }
    `)

    expect(result?.code).toContain('const fadeIn = "__nanocss_keyframes-firn26"')
    expect(result?.code).toContain(
      'const alsoFadeIn = "__nanocss_keyframes-firn26"',
    )
    expect(
      (result?.metadata as any).nanocss.styleSheet.match(/@keyframes/g),
    ).toHaveLength(1)
  })

  it('uses short generated keyframe names outside debug mode', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        const fadeIn = css.keyframes({
          '0%': {
            opacity: 0,
          },
          '100%': {
            opacity: 1,
          },
        })
      `,
      { debug: false },
    )

    expect(result?.code).toContain('const fadeIn = "nk-')
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '@keyframes nk-',
    )
  })

  it('stores generated keyframe strings in defineVars for cross-file style use', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        const fadeIn = css.keyframes({
          from: { opacity: 0 },
          to: { opacity: 1 },
        })

        export const animations = css.defineVars({
          fadeIn,
        })
      `,
      { filename: 'animations.css.ts' },
    )

    expect(result?.code).toMatchInlineSnapshot(`
      "const fadeIn = "__nanocss_keyframes-firn26";
      export const animations = {
          "fadeIn": "--_nanocss_var_animations_fade-in_vbwdqn",
          "$$defaults": {
              "--_nanocss_var_animations_fade-in_vbwdqn": "var(--_nanocss_var_animations_fade-in_vbwdqn--n-default)"
          }
      };
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "@keyframes __nanocss_keyframes-firn26 {
        from {
          opacity: 0;
        }
        to {
          opacity: 1;
        }
      }
      * {
        --_nanocss_var_animations_fade-in_vbwdqn--n-default: __nanocss_keyframes-firn26;
      }"
    `)
  })

  it('compiles positionTry and records css metadata', () => {
    const result = transformWithMetadata(`
      import { css } from 'nanocss-compiler'

      const fallback = css.positionTry({
        positionAnchor: '--anchor',
        top: '0',
        left: '0',
        width: '100px',
        height: '100px',
      })

      const fallbacks = css.defineVars({
        topLeftCorner: fallback,
      })

      const styles = css.create({
        popover: {
          positionTryFallbacks: fallback,
          positionTryOrder: fallbacks.topLeftCorner,
        },
      })

      export function Popover() {
        return <div {...css.props(styles.popover)} />
      }
    `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const fallback = "--_nanocss_position_try-b1t7hz";
      const fallbacks = {
          "topLeftCorner": "--_nanocss_var_fallbacks_top-left-corner_fusx7b",
          "$$defaults": {
              "--_nanocss_var_fallbacks_top-left-corner_fusx7b": "var(--_nanocss_var_fallbacks_top-left-corner_fusx7b--n-default)"
          }
      };
      const _stylesPopover = {
          positionTryFallbacks: fallback,
          positionTryOrder: "var(--_nanocss_var_fallbacks_top-left-corner_fusx7b, var(--_nanocss_var_fallbacks_top-left-corner_fusx7b--n-default))"
      };
      export function Popover() {
          return <div style={_stylesPopover}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "@position-try --_nanocss_position_try-b1t7hz {
        position-anchor: --anchor;
        top: 0;
        left: 0;
        width: 100px;
        height: 100px;
      }
      * {
        --_nanocss_var_fallbacks_top-left-corner_fusx7b--n-default: --_nanocss_position_try-b1t7hz;
      }"
    `)
  })

  it('uses short generated positionTry names outside debug mode', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        const fallback = css.positionTry({
          top: '0',
          left: '0',
        })
      `,
      { debug: false },
    )

    expect(result?.code).toContain('const fallback = "--npt-')
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '@position-try --npt-',
    )
  })

  it('compiles viewTransitionClass and records css metadata', () => {
    const result = transformWithMetadata(`
      import { css } from 'nanocss-compiler'

      const fadeIn = css.keyframes({
        from: {
          opacity: 0,
        },
        to: {
          opacity: 1,
        },
      })

      const colors = css.defineVars({
        primary: 'green',
      })

      const pageTransition = css.viewTransitionClass({
        group: {
          animationDuration: '250ms',
        },
        imagePair: {
          isolation: 'isolate',
        },
        old: {
          animationDuration: '120ms',
          opacity: {
            default: 1,
            '@media (prefers-reduced-motion: reduce)': 0,
          },
        },
        new: {
          animationName: fadeIn,
          animationTimingFunction: 'ease-out',
          backgroundColor: colors.primary,
          position: css.firstThatWorks('sticky', 'fixed'),
        },
      })
    `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const fadeIn = "__nanocss_keyframes-firn26";
      const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)"
          }
      };
      const pageTransition = "__nanocss_view_transition-o4gxn2";
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_media__prefers-reduced-motion__reduce_-wpcmte-0: initial;
        --_media__prefers-reduced-motion__reduce_-wpcmte-1: ;
        --_supports__position__sticky_-nb5zgb-0: initial;
        --_supports__position__sticky_-nb5zgb-1: ;
      }
      @media (prefers-reduced-motion: reduce) {
        * {
          --_media__prefers-reduced-motion__reduce_-wpcmte-0: ;
          --_media__prefers-reduced-motion__reduce_-wpcmte-1: initial;
        }
      }
      @supports (position: sticky) {
        * {
          --_supports__position__sticky_-nb5zgb-0: ;
          --_supports__position__sticky_-nb5zgb-1: initial;
        }
      }

      @keyframes __nanocss_keyframes-firn26 {
        from {
          opacity: 0;
        }
        to {
          opacity: 1;
        }
      }
      ::view-transition-group(*.__nanocss_view_transition-o4gxn2) {
        animation-duration: 250ms;
      }
      ::view-transition-image-pair(*.__nanocss_view_transition-o4gxn2) {
        isolation: isolate;
      }
      ::view-transition-old(*.__nanocss_view_transition-o4gxn2) {
        animation-duration: 120ms;
        opacity: var(--_media__prefers-reduced-motion__reduce_-wpcmte-1, 0) var(--_media__prefers-reduced-motion__reduce_-wpcmte-0, 1);
      }
      ::view-transition-new(*.__nanocss_view_transition-o4gxn2) {
        animation-name: __nanocss_keyframes-firn26;
        animation-timing-function: ease-out;
        background-color: var(--_nanocss_var_colors_primary_fbmrz5, var(--_nanocss_var_colors_primary_fbmrz5--n-default));
        position: var(--_supports__position__sticky_-nb5zgb-1, sticky) var(--_supports__position__sticky_-nb5zgb-0, fixed);
      }
      * {
        --_nanocss_var_colors_primary_fbmrz5--n-default: green;
      }"
    `)
  })

  it('uses short generated view transition class names outside debug mode', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const pageTransition = css.viewTransitionClass({
          new: {
            opacity: 1,
          },
        })
      `,
      { debug: false },
    )

    expect(result?.code).toContain('export const pageTransition = "nvt-')
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '::view-transition-new(*.nvt-',
    )
  })

  it('compiles defineVars with the custom property ABI and local createTheme calls', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const colors = css.defineVars({
          primary: 'green',
          accent: {
            default: 'blue',
            ':hover': 'red',
          },
          nested: {
            default: 'white',
            ':hover': {
              default: 'gray',
              '@media (min-width: 768px)': 'black',
            },
          },
          '--custom': '4',
        })

        const theme = css.createTheme(colors, {
          primary: 'purple',
          accent: {
            default: 'orange',
            ':hover': 'pink',
          },
          nested: {
            default: 'white',
            ':hover': {
              default: 'gray',
              '@media (min-width: 768px)': 'black',
            },
          },
          '--custom': '8',
        })

        const styles = css.create({
          root: {
            color: colors.primary,
            backgroundColor: colors.accent,
            borderTopColor: colors.nested,
          },
        })

        function Comp() {
          return <div {...css.props(theme, styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "accent": "--_nanocss_var_colors_accent_o3catn",
          "nested": "--_nanocss_var_colors_nested_uabhtk",
          "--custom": "--custom",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)",
              "--_nanocss_var_colors_accent_o3catn": "var(--_nanocss_var_colors_accent_o3catn--n-default)",
              "--_nanocss_var_colors_nested_uabhtk": "var(--_nanocss_var_colors_nested_uabhtk--n-default)",
              "--custom": "var(--custom--n-default)"
          }
      };
      const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_primary_fbmrz5": 'purple',
          "--_nanocss_var_colors_accent_o3catn": "var(--_hover-mbscpo-1, pink) var(--_hover-mbscpo-0, orange)",
          "--_nanocss_var_colors_nested_uabhtk": "var(--cond-27myt-1, black) var(--cond-27myt-0, var(--_hover-mbscpo-1, gray) var(--_hover-mbscpo-0, white))",
          "--custom": '8'
      };
      const _stylesRoot = {
          color: "var(--_nanocss_var_colors_primary_fbmrz5, var(--_nanocss_var_colors_primary_fbmrz5--n-default))",
          backgroundColor: "var(--_nanocss_var_colors_accent_o3catn, var(--_nanocss_var_colors_accent_o3catn--n-default))",
          borderTopColor: "var(--_nanocss_var_colors_nested_uabhtk, var(--_nanocss_var_colors_nested_uabhtk--n-default))"
      };
      function Comp() {
          return <div style={{
              ...theme,
              ..._stylesRoot
          }}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '--_nanocss_var_colors_primary_fbmrz5--n-default: green;',
    )
  })

  it('compiles typed variables and typed theme overrides', () => {
    const result = transformWithMetadata(`
      import { css } from 'nanocss-compiler'

      const tokens = css.defineVars({
        accent: css.types.color({
          default: 'blue',
          ':hover': 'red',
        }),
        space: css.types.length(4),
      })

      const theme = css.createTheme(tokens, {
        accent: css.types.color({
          default: 'purple',
          '@media (prefers-color-scheme: dark)': 'plum',
        }),
        space: css.types.length(8),
      })

      const styles = css.create({
        root: {
          color: tokens.accent,
          marginTop: tokens.space,
        },
      })

      function Comp() {
        return <div {...css.props(theme, styles.root)} />
      }
    `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const tokens = {
          "accent": "--_nanocss_var_tokens_accent_gubwwx",
          "space": "--_nanocss_var_tokens_space_tgs233",
          "$$defaults": {
              "--_nanocss_var_tokens_accent_gubwwx": "var(--_nanocss_var_tokens_accent_gubwwx--n-default)",
              "--_nanocss_var_tokens_space_tgs233": "var(--_nanocss_var_tokens_space_tgs233--n-default)"
          }
      };
      const theme = {
          ...tokens.$$defaults,
          "--_nanocss_var_tokens_accent_gubwwx": "var(--_media__prefers-color-scheme__dark_-i73cho-1, plum) var(--_media__prefers-color-scheme__dark_-i73cho-0, purple)",
          "--_nanocss_var_tokens_space_tgs233": "8px"
      };
      const _stylesRoot = {
          color: "var(--_nanocss_var_tokens_accent_gubwwx, var(--_nanocss_var_tokens_accent_gubwwx--n-default))",
          marginTop: "var(--_nanocss_var_tokens_space_tgs233, var(--_nanocss_var_tokens_space_tgs233--n-default))"
      };
      function Comp() {
          return <div style={{
              ...theme,
              ..._stylesRoot
          }}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_hover-mbscpo-0: initial;
        --_hover-mbscpo-1: ;
        --_media__prefers-color-scheme__dark_-i73cho-0: initial;
        --_media__prefers-color-scheme__dark_-i73cho-1: ;
      }
      *:hover {
        --_hover-mbscpo-0: ;
        --_hover-mbscpo-1: initial;
      }
      @media (prefers-color-scheme: dark) {
        * {
          --_media__prefers-color-scheme__dark_-i73cho-0: ;
          --_media__prefers-color-scheme__dark_-i73cho-1: initial;
        }
      }

      @property --_nanocss_var_tokens_accent_gubwwx {
        syntax: "<color>";
        inherits: true;
        initial-value: blue;
      }
      @property --_nanocss_var_tokens_space_tgs233 {
        syntax: "<length>";
        inherits: true;
        initial-value: 4px;
      }
      * {
        --_nanocss_var_tokens_accent_gubwwx--n-default: var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, blue);
        --_nanocss_var_tokens_accent_gubwwx: var(--_nanocss_var_tokens_accent_gubwwx--n-default);
        --_nanocss_var_tokens_space_tgs233--n-default: 4px;
        --_nanocss_var_tokens_space_tgs233: var(--_nanocss_var_tokens_space_tgs233--n-default);
      }"
    `)
  })

  it('records condition vars for exported nested defineVars metadata', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const colors = css.defineVars({
          primary: 'green',
          nested: {
            default: 'white',
            ':hover': {
              default: 'gray',
              '@media (min-width: 768px)': 'black',
            },
          },
        })
      `,
      { filename: 'src/colors.css.ts' },
    )

    expect(result?.code).toContain('"nested": "--_nanocss_var_')
    const primary = result?.code.match(/"primary": "(--_nanocss_var_[^"]+)"/)?.[1]
    const nested = result?.code.match(/"nested": "(--_nanocss_var_[^"]+)"/)?.[1]
    const styleSheet = (result?.metadata as any).nanocss.styleSheet
    expect(primary).toBeTruthy()
    expect(nested).toBeTruthy()
    expect(styleSheet).toContain(
      '--cond-27myt-0: var(--_hover-mbscpo-0) var(--_media__min-width__768px_-apgmjb-0);',
    )
    expect(styleSheet).toContain(
      '--cond-27myt-1: var(--_hover-mbscpo-1, var(--_media__min-width__768px_-apgmjb-1));',
    )
    expect(styleSheet).toContain(
      `${primary}--n-default: green;`,
    )
    expect(styleSheet).toContain(
      `${nested}--n-default: var(--cond-27myt-1, black)`,
    )
    expect(styleSheet).not.toContain(`${primary}: green;`)
  })

  it('uses short variable default names outside debug mode', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const colors = css.defineVars({
          primary: 'green',
        })
      `,
      { debug: false, filename: 'src/colors.css.ts' },
    )
    const primary = result?.code.match(/"primary": "(--nv-[^"]+)"/)?.[1]
    const styleSheet = (result?.metadata as any).nanocss.styleSheet

    expect(primary).toBeTruthy()
    expect(styleSheet).toContain(`${primary}--nd:green;`)
    expect(styleSheet).not.toContain(`${primary}--n-default: green;`)
  })

  it('uses short generated variable names outside debug mode', () => {
    expect(
      transform(
        `
          import { css } from 'nanocss-compiler'

          export const colors = css.defineVars({
            primary: 'green',
          })
        `,
        { debug: false, filename: 'src/colors.css.ts' },
      ),
    ).toMatchInlineSnapshot(`
      "export const colors = {
          "primary": "--nv-468dmp",
          "$$defaults": {
              "--nv-468dmp": "var(--nv-468dmp--nd)"
          }
      };
      "
    `)
  })

  it('allows exported defineVars and createTheme in css module files', () => {
    expect(
      transform(
        `
          import { css } from 'nanocss-compiler'

          export const colors = css.defineVars({
            primary: 'green',
          })

          export const theme = css.createTheme(colors, {
            primary: 'purple',
          })
        `,
        { filename: 'src/colors.css.ts' },
      ),
    ).toMatchInlineSnapshot(`
      "export const colors = {
          "primary": "--_nanocss_var_colors_primary_468dmp",
          "$$defaults": {
              "--_nanocss_var_colors_primary_468dmp": "var(--_nanocss_var_colors_primary_468dmp--n-default)"
          }
      };
      export const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_primary_468dmp": 'purple'
      };
      "
    `)
  })

  it('compiles exported defineConsts to frozen runtime constants', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const tokens = css.defineConsts({
          compact: '@media (max-width: 700px)',
          sticky: 'sticky',
          swatchSize: 32,
        })
      `,
      { filename: 'src/tokens.css.ts' },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "export const tokens = Object.freeze({
          compact: '@media (max-width: 700px)',
          sticky: 'sticky',
          swatchSize: 32
      });
      "
    `)
  })

  it('resolves css.env JSON values at compile time', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const constants = css.defineConsts({
          compact: css.env.compact,
          dashed: css.env['foo-bar'],
          accent: css.env.colors.accent,
          swatchSize: css.env.swatchSize,
        })

        const colors = css.defineVars(css.env.vars)
        const theme = css.createTheme(colors, css.env.theme)

        const fade = css.keyframes({
          from: {
            opacity: 0,
            backgroundColor: css.env.colors.accent,
          },
          to: {
            opacity: 1,
            backgroundColor: css.env.colors.dark,
          },
        })

        const styles = css.create({
          root: {
            [css.env.customProperty]: css.env.swatchSize,
            animationName: fade,
            color: css.env.colors.accent,
            width: css.env.swatchSize,
            position: css.firstThatWorks(css.env.sticky, 'fixed'),
            backgroundColor: {
              default: css.env.colors.accent,
              [css.env.compact]: css.env.colors.dark,
            },
          },
        })

        export function App() {
          return <div {...css.props(theme, styles.root)} />
        }
      `,
      {
        filename: 'src/tokens.css.tsx',
        env: {
          compact: '@media (max-width: 700px)',
          customProperty: '--swatch-size',
          'foo-bar': 'dashed-value',
          sticky: 'sticky',
          swatchSize: 32,
          colors: {
            accent: '#c04f2f',
            dark: '#4a1b12',
          },
          vars: {
            accent: '#111',
          },
          theme: {
            accent: '#c04f2f',
          },
        },
      },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "export const constants = Object.freeze({
          compact: "@media (max-width: 700px)",
          dashed: "dashed-value",
          accent: "#c04f2f",
          swatchSize: 32
      });
      const colors = {
          "accent": "--_nanocss_var_colors_accent_zhggz7",
          "$$defaults": {
              "--_nanocss_var_colors_accent_zhggz7": "var(--_nanocss_var_colors_accent_zhggz7--n-default)"
          }
      };
      const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_accent_zhggz7": "#c04f2f"
      };
      const fade = "__nanocss_keyframes-qk16ac";
      const _stylesRoot = {
          "--swatch-size": 32,
          animationName: fade,
          color: "#c04f2f",
          width: 32,
          position: "var(--_supports__position__sticky_-nb5zgb-1, sticky) var(--_supports__position__sticky_-nb5zgb-0, fixed)",
          backgroundColor: "var(--_media__max-width__700px_-sdukub-1, #4a1b12) var(--_media__max-width__700px_-sdukub-0, #c04f2f)"
      };
      export function App() {
          return <div style={{
              ...theme,
              ..._stylesRoot
          }}/>;
      }
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_supports__position__sticky_-nb5zgb-0: initial;
        --_supports__position__sticky_-nb5zgb-1: ;
        --_media__max-width__700px_-sdukub-0: initial;
        --_media__max-width__700px_-sdukub-1: ;
      }
      @supports (position: sticky) {
        * {
          --_supports__position__sticky_-nb5zgb-0: ;
          --_supports__position__sticky_-nb5zgb-1: initial;
        }
      }
      @media (max-width: 700px) {
        * {
          --_media__max-width__700px_-sdukub-0: ;
          --_media__max-width__700px_-sdukub-1: initial;
        }
      }

      @keyframes __nanocss_keyframes-qk16ac {
        from {
          opacity: 0;
          background-color: #c04f2f;
        }
        to {
          opacity: 1;
          background-color: #4a1b12;
        }
      }
      * {
        --_nanocss_var_colors_accent_zhggz7--n-default: #111;
      }"
    `)
  })

  it('throws when css.env values are called', () => {
    expect(() =>
      transform(
        `
          import { css } from 'nanocss-compiler'

          const styles = css.create({
            root: {
              color: css.env.colorMix('red', 'blue', 50),
            },
          })
        `,
        {
          env: {
            colorMix: 'color-mix',
          },
        },
      ),
    ).toThrow('[nanocss] css.env values cannot be called.')
  })

  it('compiles derived defineVars defaults from same-file references', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const base = css.defineVars({
          overlay: 'rgba(0 0 0 / 0.3)',
        })

        export const constants = css.defineConsts({
          tone: 'white',
          pct: 15,
        })

        export const colors = css.defineVars({
          accent: css.types.color('red'),
          typedAccent: () => css.types.color('blue'),
          textMuted: () => \`color-mix(\${colors.text}, \${base.overlay} 20%)\`,
          accentGlow: () => \`color-mix(\${colors.accent}, \${constants.tone} \${constants.pct}%)\`,
          text: 'black',
        })
      `,
      { filename: 'src/tokens.css.ts' },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "export const base = {
          "overlay": "--_nanocss_var_base_overlay_1ixz4q",
          "$$defaults": {
              "--_nanocss_var_base_overlay_1ixz4q": "var(--_nanocss_var_base_overlay_1ixz4q--n-default)"
          }
      };
      export const constants = Object.freeze({
          tone: 'white',
          pct: 15
      });
      export const colors = {
          "accent": "--_nanocss_var_colors_accent_6a3xtt",
          "typedAccent": "--_nanocss_var_colors_typed-accent_ycd5h9",
          "textMuted": "--_nanocss_var_colors_text-muted_y1wwiv",
          "accentGlow": "--_nanocss_var_colors_accent-glow_c622we",
          "text": "--_nanocss_var_colors_text_prw5d0",
          "$$defaults": {
              "--_nanocss_var_colors_accent_6a3xtt": "var(--_nanocss_var_colors_accent_6a3xtt--n-default)",
              "--_nanocss_var_colors_typed-accent_ycd5h9": "var(--_nanocss_var_colors_typed-accent_ycd5h9--n-default)",
              "--_nanocss_var_colors_text-muted_y1wwiv": "var(--_nanocss_var_colors_text-muted_y1wwiv--n-default)",
              "--_nanocss_var_colors_accent-glow_c622we": "var(--_nanocss_var_colors_accent-glow_c622we--n-default)",
              "--_nanocss_var_colors_text_prw5d0": "var(--_nanocss_var_colors_text_prw5d0--n-default)"
          }
      };
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toMatchInlineSnapshot(`
      "@property --_nanocss_var_colors_accent_6a3xtt {
        syntax: "<color>";
        inherits: true;
        initial-value: red;
      }
      @property --_nanocss_var_colors_typed-accent_ycd5h9 {
        syntax: "<color>";
        inherits: true;
        initial-value: blue;
      }
      * {
        --_nanocss_var_base_overlay_1ixz4q--n-default: rgba(0 0 0 / 0.3);
        --_nanocss_var_colors_accent_6a3xtt--n-default: red;
        --_nanocss_var_colors_accent_6a3xtt: var(--_nanocss_var_colors_accent_6a3xtt--n-default);
        --_nanocss_var_colors_typed-accent_ycd5h9--n-default: blue;
        --_nanocss_var_colors_typed-accent_ycd5h9: var(--_nanocss_var_colors_typed-accent_ycd5h9--n-default);
        --_nanocss_var_colors_text-muted_y1wwiv--n-default: color-mix(var(--_nanocss_var_colors_text_prw5d0, var(--_nanocss_var_colors_text_prw5d0--n-default)), var(--_nanocss_var_base_overlay_1ixz4q, var(--_nanocss_var_base_overlay_1ixz4q--n-default)) 20%);
        --_nanocss_var_colors_accent-glow_c622we--n-default: color-mix(var(--_nanocss_var_colors_accent_6a3xtt, var(--_nanocss_var_colors_accent_6a3xtt--n-default)), white 15%);
        --_nanocss_var_colors_text_prw5d0--n-default: black;
      }"
    `)
  })

  it('throws when derived defineVars defaults reference imported variable groups', () => {
    expect(() =>
      transform(
        `
          import { css } from 'nanocss-compiler'
          import { colors } from './colors.css'

          export const tokens = css.defineVars({
            muted: () => \`color-mix(\${colors.text}, transparent 50%)\`,
          })
        `,
        { filename: 'src/tokens.css.ts' },
      ),
    ).toThrow(
      '[nanocss] css.defineVars(...) function values cannot reference imported variable groups.',
    )
  })

  it('throws when derived defineVars defaults create same-group cycles', () => {
    expect(() =>
      transform(
        `
          import { css } from 'nanocss-compiler'

          export const colors = css.defineVars({
            a: () => colors.b,
            b: () => colors.c,
            c: () => colors.a,
          })
        `,
        { filename: 'src/tokens.css.ts' },
      ),
    ).toThrow(
      '[nanocss] Cyclic same-group references in css.defineVars(...) are not allowed: a -> b -> c -> a.',
    )
  })

  it('inlines local defineConsts values in style values and computed hook keys', () => {
    const result = transformWithMetadata(
      `
        import { css } from 'nanocss-compiler'

        export const tokens = css.defineConsts({
          compact: '@media (max-width: 700px)',
          accent: '#c04f2f',
          sticky: 'sticky',
          swatchSize: 32,
        })

        const colors = css.defineVars({
          accent: '#111',
        })

        const theme = css.createTheme(colors, {
          accent: {
            default: tokens.accent,
            [tokens.compact]: '#7a2716',
          },
        })

        const styles = css.create({
          root: {
            width: tokens.swatchSize,
            position: css.firstThatWorks(tokens.sticky, 'fixed'),
            color: {
              default: 'black',
              [tokens.compact]: 'red',
            },
          },
        })

        export function App() {
          return <div {...css.props(theme, styles.root)} />
        }
      `,
      { filename: 'src/tokens.css.tsx' },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "export const tokens = Object.freeze({
          compact: '@media (max-width: 700px)',
          accent: '#c04f2f',
          sticky: 'sticky',
          swatchSize: 32
      });
      const colors = {
          "accent": "--_nanocss_var_colors_accent_zhggz7",
          "$$defaults": {
              "--_nanocss_var_colors_accent_zhggz7": "var(--_nanocss_var_colors_accent_zhggz7--n-default)"
          }
      };
      const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_accent_zhggz7": "var(--_media__max-width__700px_-sdukub-1, #7a2716) var(--_media__max-width__700px_-sdukub-0, #c04f2f)"
      };
      const _stylesRoot = {
          width: 32,
          position: "var(--_supports__position__sticky_-nb5zgb-1, sticky) var(--_supports__position__sticky_-nb5zgb-0, fixed)",
          color: "var(--_media__max-width__700px_-sdukub-1, red) var(--_media__max-width__700px_-sdukub-0, black)"
      };
      export function App() {
          return <div style={{
              ...theme,
              ..._stylesRoot
          }}/>;
      }
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_media__max-width__700px_-sdukub-0: initial;
        --_media__max-width__700px_-sdukub-1: ;
        --_supports__position__sticky_-nb5zgb-0: initial;
        --_supports__position__sticky_-nb5zgb-1: ;
      }
      @media (max-width: 700px) {
        * {
          --_media__max-width__700px_-sdukub-0: ;
          --_media__max-width__700px_-sdukub-1: initial;
        }
      }
      @supports (position: sticky) {
        * {
          --_supports__position__sticky_-nb5zgb-0: ;
          --_supports__position__sticky_-nb5zgb-1: initial;
        }
      }

      * {
        --_nanocss_var_colors_accent_zhggz7--n-default: #111;
      }"
    `)
  })

  it('compiles nested createTheme overrides with condition vars', () => {
    const result = transformWithMetadata(`
      import { css } from 'nanocss-compiler'

      const colors = css.defineVars({
        nested: {
          default: 'white',
          ':hover': {
            default: 'gray',
            '@media (min-width: 768px)': 'black',
          },
        },
      })

      const theme = css.createTheme(colors, {
        nested: {
          default: 'linen',
          ':hover': {
            default: 'silver',
            '@media (min-width: 768px)': 'navy',
          },
        },
      })

      function Comp() {
        return <div {...css.props(theme)} />
      }
    `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const colors = {
          "nested": "--_nanocss_var_colors_nested_uabhtk",
          "$$defaults": {
              "--_nanocss_var_colors_nested_uabhtk": "var(--_nanocss_var_colors_nested_uabhtk--n-default)"
          }
      };
      const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_nested_uabhtk": "var(--cond-27myt-1, navy) var(--cond-27myt-0, var(--_hover-mbscpo-1, silver) var(--_hover-mbscpo-0, linen))"
      };
      function Comp() {
          return <div style={theme}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '--cond-27myt-0: var(--_hover-mbscpo-0) var(--_media__min-width__768px_-apgmjb-0);',
    )
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '--cond-27myt-1: var(--_hover-mbscpo-1, var(--_media__min-width__768px_-apgmjb-1));',
    )
  })

  it('compiles createTheme for imported variable groups using the custom property ABI', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const theme = css.createTheme(colors, {
          primary: 'purple',
          danger: null,
        })
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const theme = {
          ...colors.$$defaults,
          [colors.primary]: 'purple',
          [colors.danger]: void 0
      };
      "
    `)
  })

  it('compiles empty createTheme overrides to a reset theme', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const colors = css.defineVars({
          primary: 'green',
          accent: 'blue',
        })

        const reset = css.createTheme(colors, {})
      `),
    ).toMatchInlineSnapshot(`
      "const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "accent": "--_nanocss_var_colors_accent_o3catn",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)",
              "--_nanocss_var_colors_accent_o3catn": "var(--_nanocss_var_colors_accent_o3catn--n-default)"
          }
      };
      const reset = {
          ...colors.$$defaults
      };
      "
    `)
  })

  it('resets earlier same-group themes when themes are merged', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const colors = css.defineVars({
          primary: 'green',
          accent: 'blue',
        })

        const primaryTheme = css.createTheme(colors, {
          primary: 'purple',
        })

        const accentTheme = css.createTheme(colors, {
          accent: 'orange',
        })

        function Comp() {
          return <div {...css.props(primaryTheme, accentTheme)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "accent": "--_nanocss_var_colors_accent_o3catn",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)",
              "--_nanocss_var_colors_accent_o3catn": "var(--_nanocss_var_colors_accent_o3catn--n-default)"
          }
      };
      const primaryTheme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_primary_fbmrz5": 'purple'
      };
      const accentTheme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_accent_o3catn": 'orange'
      };
      function Comp() {
          return <div style={{
              ...primaryTheme,
              ...accentTheme
          }}/>;
      }
      "
    `)
  })

  it('compiles computed local variable token keys in style objects', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const colors = css.defineVars({
          primary: 'green',
        })

        const styles = css.create({
          root: {
            [colors.primary]: {
              default: 'red',
              ':hover': 'blue',
            },
            color: colors.primary,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)"
          }
      };
      const _stylesRoot = {
          "--_nanocss_var_colors_primary_fbmrz5": "var(--_hover-mbscpo-1, blue) var(--_hover-mbscpo-0, red)",
          color: "var(--_nanocss_var_colors_primary_fbmrz5, var(--_nanocss_var_colors_primary_fbmrz5--n-default))"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('compiles computed imported variable token keys in style objects', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const styles = css.create({
          root: {
            [colors.primary]: 'red',
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _stylesRoot = {
          [colors.primary]: 'red'
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('compiles hook objects for computed imported variable token keys', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const styles = css.create({
          root: {
            [colors.primary]: {
              default: 'red',
              ':hover': 'blue',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _stylesRoot = {
          [colors.primary]: "var(--_hover-mbscpo-1, blue) var(--_hover-mbscpo-0, red)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('preserves computed imported variable token keys when merging static styles', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const styles = css.create({
          a: {
            [colors.primary]: 'red',
          },
          b: {
            color: 'blue',
          },
        })

        function Comp() {
          return <div {...css.props(styles.a, styles.b)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _stylesA = {
          [colors.primary]: 'red'
      };
      const _stylesB = {
          color: 'blue'
      };
      function Comp() {
          return <div style={{
              ..._stylesA,
              ..._stylesB
          }}/>;
      }
      "
    `)
  })

  it('compiles variable tokens inside hook values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const colors = css.defineVars({
          primary: 'green',
          accent: 'red',
        })

        const styles = css.create({
          root: {
            color: {
              default: colors.primary,
              ':hover': colors.accent,
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const colors = {
          "primary": "--_nanocss_var_colors_primary_fbmrz5",
          "accent": "--_nanocss_var_colors_accent_o3catn",
          "$$defaults": {
              "--_nanocss_var_colors_primary_fbmrz5": "var(--_nanocss_var_colors_primary_fbmrz5--n-default)",
              "--_nanocss_var_colors_accent_o3catn": "var(--_nanocss_var_colors_accent_o3catn--n-default)"
          }
      };
      const _stylesRoot = {
          color: "var(--_hover-mbscpo-1, var(--_nanocss_var_colors_accent_o3catn, var(--_nanocss_var_colors_accent_o3catn--n-default))) var(--_hover-mbscpo-0, var(--_nanocss_var_colors_primary_fbmrz5, var(--_nanocss_var_colors_primary_fbmrz5--n-default)))"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('compiles imported variable tokens inside hook values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const styles = css.create({
          root: {
            color: {
              default: colors.primary,
              ':hover': colors.accent,
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _stylesRoot = {
          "--_nanocss_dynamic_onc9m5": "var(" + colors.primary + ", var(" + (colors.primary + "--n-default") + "))",
          "--_nanocss_dynamic_onc9n0": "var(" + colors.accent + ", var(" + (colors.accent + "--n-default") + "))",
          color: "var(--_hover-mbscpo-1, var(--_nanocss_dynamic_onc9n0, var(--_nanocss_dynamic_onc9m5))) var(--_hover-mbscpo-0, var(--_nanocss_dynamic_onc9m5))"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('uses short imported variable fallback names outside debug mode', () => {
    expect(
      transform(
        `
          import { css } from 'nanocss-compiler'
          import { colors } from './colors.css'

          const styles = css.create({
            root: {
              color: colors.primary,
            },
          })

          function Comp() {
            return <div {...css.props(styles.root)} />
          }
        `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _styles = {
          color: "var(" + colors.primary + ", var(" + (colors.primary + "--nd") + "))"
      };
      function Comp() {
          return <div style={_styles}/>;
      }
      "
    `)
  })

  it('uses short dynamic hook names outside debug mode', () => {
    expect(
      transform(
        `
          import { css } from 'nanocss-compiler'

          const styles = css.create({
            root: width => ({
              marginLeft: {
                default: 0,
                ':hover': width,
              },
            }),
          })

          function Comp({ width }) {
            return <div {...css.props(styles.root(width))} />
          }
        `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = (width)=>({
              "--nd-onc9m5": typeof width === "number" ? width + "px" : width,
              marginLeft: "var(--mbscpo-1,var(--nd-onc9m5, 0px))var(--mbscpo-0,0px)"
          });
      function Comp({ width }) {
          return <div style={_stylesRoot(width)}/>;
      }
      "
    `)
  })

  it('does not treat css-like import path substrings as compiled css modules', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { tokens } from './foo.css.helpers'

        const styles = css.create({
          root: {
            color: tokens.primary,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { tokens } from './foo.css.helpers';
      const _stylesRoot = {
          color: tokens.primary
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('preserves null resets in static styles and hooks', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          a: {
            color: 'red',
          },
          b: {
            color: null,
          },
          c: {
            backgroundColor: {
              default: 'red',
              ':hover': null,
            },
          },
        })

        function Comp() {
          return (
            <>
              <div {...css.props(styles.a, styles.b)} />
              <span {...css.props(styles.c)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesA = {
          color: 'red'
      };
      const _stylesB = {
          color: null
      };
      const _stylesC = {
          backgroundColor: "var(--_hover-mbscpo-1, revert-layer) var(--_hover-mbscpo-0, red)"
      };
      function Comp() {
          return <>
                    <div style={{
              ..._stylesA,
              ..._stylesB
          }}/>
                    <span style={_stylesC}/>
                  </>;
      }
      "
    `)
  })

  it('serializes numeric hook values with property-aware units', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            marginLeft: {
              default: 0,
              ':hover': 10,
            },
            opacity: {
              default: 0,
              ':hover': 1,
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          marginLeft: "var(--_hover-mbscpo-1, 10px) var(--_hover-mbscpo-0, 0px)",
          opacity: "var(--_hover-mbscpo-1, 1) var(--_hover-mbscpo-0, 0)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('precomputes media and nested hook conditions', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              '@media (min-width: 768px)': 'red',
            },
            backgroundColor: {
              default: 'white',
              ':hover': {
                default: 'gray',
                '@media (min-width: 768px)': 'blue',
              },
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_media__min-width__768px_-apgmjb-1, red) var(--_media__min-width__768px_-apgmjb-0, black)",
          backgroundColor: "var(--cond-27myt-1, blue) var(--cond-27myt-0, var(--_hover-mbscpo-1, gray) var(--_hover-mbscpo-0, white))"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
        "* {
          --_media__min-width__768px_-apgmjb-0: initial;
          --_media__min-width__768px_-apgmjb-1: ;
          --_hover-mbscpo-0: initial;
          --_hover-mbscpo-1: ;
          --cond-27myt-0: var(--_hover-mbscpo-0) var(--_media__min-width__768px_-apgmjb-0);
          --cond-27myt-1: var(--_hover-mbscpo-1, var(--_media__min-width__768px_-apgmjb-1));
        }
        @media (min-width: 768px) {
          * {
            --_media__min-width__768px_-apgmjb-0: ;
            --_media__min-width__768px_-apgmjb-1: initial;
          }
        }
        *:hover {
          --_hover-mbscpo-0: ;
          --_hover-mbscpo-1: initial;
        }
        "
      `)
  })

  it('omits hook output for no-op conditional style values', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'red',
              ':hover': 'red',
            },
            backgroundColor: {
              default: 'white',
              ':hover': {
                default: 'white',
                '@media (min-width: 768px)': 'blue',
              },
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "red",
          backgroundColor: "var(--cond-27myt-1, blue) var(--cond-27myt-0, white)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_hover-mbscpo-0: initial;
        --_hover-mbscpo-1: ;
        --_media__min-width__768px_-apgmjb-0: initial;
        --_media__min-width__768px_-apgmjb-1: ;
        --cond-27myt-0: var(--_hover-mbscpo-0) var(--_media__min-width__768px_-apgmjb-0);
        --cond-27myt-1: var(--_hover-mbscpo-1, var(--_media__min-width__768px_-apgmjb-1));
      }
      *:hover {
        --_hover-mbscpo-0: ;
        --_hover-mbscpo-1: initial;
      }
      @media (min-width: 768px) {
        * {
          --_media__min-width__768px_-apgmjb-0: ;
          --_media__min-width__768px_-apgmjb-1: initial;
        }
      }
      "
    `)
  })

  it('infers supports hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              '@supports (display: grid)': 'red',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_supports__display__grid_-vkqnq5-1, red) var(--_supports__display__grid_-vkqnq5-0, black)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toContain(
      '@supports (display: grid)',
    )
  })

  it('compiles supported pseudo selector hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':focus-visible': 'red',
              ':is([data-disabled], [aria-disabled="true"])': 'gray',
            },
            opacity: {
              default: 1,
              ':nth-child(2n + 1)': 0.6,
            },
            backgroundColor: {
              default: 'white',
              ':disabled': 'gray',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_is__data-disabled____aria-disabled__true___-qvg30p-1, gray) var(--_is__data-disabled____aria-disabled__true___-qvg30p-0, var(--_focus-visible-bb83zv-1, red) var(--_focus-visible-bb83zv-0, black))",
          opacity: "var(--_nth-child_2n___1_-aafyv2-1, 0.6) var(--_nth-child_2n___1_-aafyv2-0, 1)",
          backgroundColor: "var(--_disabled-u6v7g0-1, gray) var(--_disabled-u6v7g0-0, white)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_focus-visible-bb83zv-0: initial;
        --_focus-visible-bb83zv-1: ;
        --_is__data-disabled____aria-disabled__true___-qvg30p-0: initial;
        --_is__data-disabled____aria-disabled__true___-qvg30p-1: ;
        --_nth-child_2n___1_-aafyv2-0: initial;
        --_nth-child_2n___1_-aafyv2-1: ;
        --_disabled-u6v7g0-0: initial;
        --_disabled-u6v7g0-1: ;
      }
      *:focus-visible {
        --_focus-visible-bb83zv-0: ;
        --_focus-visible-bb83zv-1: initial;
      }
      *:is([data-disabled], [aria-disabled="true"]) {
        --_is__data-disabled____aria-disabled__true___-qvg30p-0: ;
        --_is__data-disabled____aria-disabled__true___-qvg30p-1: initial;
      }
      *:nth-child(2n + 1) {
        --_nth-child_2n___1_-aafyv2-0: ;
        --_nth-child_2n___1_-aafyv2-1: initial;
      }
      *:disabled {
        --_disabled-u6v7g0-0: ;
        --_disabled-u6v7g0-1: initial;
      }
      "
    `)
  })

  it('compiles attribute selector hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: {
              default: 1,
              '[data-disabled]': 0.5,
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: "var(--_data-disabled_-gj31wt-1, 0.5) var(--_data-disabled_-gj31wt-0, 1)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_data-disabled_-gj31wt-0: initial;
        --_data-disabled_-gj31wt-1: ;
      }
      *[data-disabled] {
        --_data-disabled_-gj31wt-0: ;
        --_data-disabled_-gj31wt-1: initial;
      }
      "
    `)
  })

  it('infers container hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              '@container card (min-width: 300px)': 'red',
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          color: "var(--_container_card__min-width__300px_-vmitvc-1, red) var(--_container_card__min-width__300px_-vmitvc-0, black)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_container_card__min-width__300px_-vmitvc-0: initial;
        --_container_card__min-width__300px_-vmitvc-1: ;
      }
      @container card (min-width: 300px) {
        * {
          --_container_card__min-width__300px_-vmitvc-0: ;
          --_container_card__min-width__300px_-vmitvc-1: initial;
        }
      }
      "
    `)
  })

  it('infers starting-style hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: {
              default: 1,
              '@starting-style': 0,
            },
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: "var(--_starting-style-qm2js0-1, 0) var(--_starting-style-qm2js0-0, 1)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_starting-style-qm2js0-0: initial;
        --_starting-style-qm2js0-1: ;
      }
      @starting-style {
        * {
          --_starting-style-qm2js0-0: ;
          --_starting-style-qm2js0-1: initial;
        }
      }
      "
    `)
  })

  it('compiles firstThatWorks fallback values through supports hooks', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            position: css.firstThatWorks('sticky', '-webkit-sticky', 'fixed'),
            width: css.firstThatWorks('max-content', 320),
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          position: "var(--_supports__position__sticky_-nb5zgb-1, sticky) var(--_supports__position__sticky_-nb5zgb-0, var(--_supports__position__-webkit-sticky_-ddfgpl-1, -webkit-sticky) var(--_supports__position__-webkit-sticky_-ddfgpl-0, fixed))",
          width: "var(--_supports__width__max-content_-7e773b-1, max-content) var(--_supports__width__max-content_-7e773b-0, 320px)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_supports__position__-webkit-sticky_-ddfgpl-0: initial;
        --_supports__position__-webkit-sticky_-ddfgpl-1: ;
        --_supports__position__sticky_-nb5zgb-0: initial;
        --_supports__position__sticky_-nb5zgb-1: ;
        --_supports__width__max-content_-7e773b-0: initial;
        --_supports__width__max-content_-7e773b-1: ;
      }
      @supports (position: -webkit-sticky) {
        * {
          --_supports__position__-webkit-sticky_-ddfgpl-0: ;
          --_supports__position__-webkit-sticky_-ddfgpl-1: initial;
        }
      }
      @supports (position: sticky) {
        * {
          --_supports__position__sticky_-nb5zgb-0: ;
          --_supports__position__sticky_-nb5zgb-1: initial;
        }
      }
      @supports (width: max-content) {
        * {
          --_supports__width__max-content_-7e773b-0: ;
          --_supports__width__max-content_-7e773b-1: initial;
        }
      }
      "
    `)
  })

  it('supports aliased firstThatWorks calls', () => {
    const result = transformWithMetadata(`
        import { css as c } from 'nanocss-compiler'

        const styles = c.create({
          root: {
            display: c.firstThatWorks('grid', 'flex'),
          },
        })

        function Comp() {
          return <div {...c.props(styles.root)} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          display: "var(--_supports__display__grid_-vkqnq5-1, grid) var(--_supports__display__grid_-vkqnq5-0, flex)"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_supports__display__grid_-vkqnq5-0: initial;
        --_supports__display__grid_-vkqnq5-1: ;
      }
      @supports (display: grid) {
        * {
          --_supports__display__grid_-vkqnq5-0: ;
          --_supports__display__grid_-vkqnq5-1: initial;
        }
      }
      "
    `)
  })

  it('compiles firstThatWorks inside dynamic style functions', () => {
    const result = transformWithMetadata(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: (width) => ({
            width,
            position: css.firstThatWorks('sticky', '-webkit-sticky', 'fixed'),
          }),
        })

        function Comp({ width }) {
          return <div {...css.props(styles.root(width))} />
        }
      `)

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = (width)=>({
              width,
              position: "var(--_supports__position__sticky_-nb5zgb-1, sticky) var(--_supports__position__sticky_-nb5zgb-0, var(--_supports__position__-webkit-sticky_-ddfgpl-1, -webkit-sticky) var(--_supports__position__-webkit-sticky_-ddfgpl-0, fixed))"
          });
      function Comp({ width }) {
          return <div style={_stylesRoot(width)}/>;
      }
      "
    `)
    expect((result?.metadata as any).nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_supports__position__-webkit-sticky_-ddfgpl-0: initial;
        --_supports__position__-webkit-sticky_-ddfgpl-1: ;
        --_supports__position__sticky_-nb5zgb-0: initial;
        --_supports__position__sticky_-nb5zgb-1: ;
      }
      @supports (position: -webkit-sticky) {
        * {
          --_supports__position__-webkit-sticky_-ddfgpl-0: ;
          --_supports__position__-webkit-sticky_-ddfgpl-1: initial;
        }
      }
      @supports (position: sticky) {
        * {
          --_supports__position__sticky_-nb5zgb-0: ;
          --_supports__position__sticky_-nb5zgb-1: initial;
        }
      }
      "
    `)
  })

  it('compiles mixed dynamic props and local static styles', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          x: {
            marginLeft: 0,
          },
        })

        function Comp({ style }) {
          return <div {...css.props(style, styles.x)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesX = {
          marginLeft: 0
      };
      function Comp({ style }) {
          return <div style={{
              ...style,
              ..._stylesX
          }}/>;
      }
      "
    `)
  })

  it('compiles dynamic style functions', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            marginLeft: 0,
          },
          foo: (width, color, opacity) => ({
            width,
            color,
            opacity,
          }),
        })

        function Comp({ style, width, color, opacity }) {
          return <div {...css.props(style, styles.foo(width, color, opacity), styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesFoo = (width, color, opacity)=>({
              width,
              color,
              opacity
          });
      const _stylesRoot = {
          marginLeft: 0
      };
      function Comp({ style, width, color, opacity }) {
          return <div style={{
              ...style,
              ..._stylesFoo(width, color, opacity),
              ..._stylesRoot
          }}/>;
      }
      "
    `)
  })

  it('reuses dynamic style helpers across call sites', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          swatch: backgroundColor => ({
            width: 32,
            height: 32,
            backgroundColor,
          }),
        })

        function Comp() {
          return (
            <>
              <div {...css.props(styles.swatch('#70c7b5'))} />
              <div {...css.props(styles.swatch('#ef8d6f'))} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesSwatch = (backgroundColor)=>({
              width: 32,
              height: 32,
              backgroundColor
          });
      function Comp() {
          return <>
                    <div style={_stylesSwatch('#70c7b5')}/>
                    <div style={_stylesSwatch('#ef8d6f')}/>
                  </>;
      }
      "
    `)
  })

  it('throws when dynamic style function parameters are not simple identifiers', () => {
    const cases = [
      'root: ({ height }) => ({ height })',
      'root: (height = 10) => ({ height })',
      'root: (...values) => ({ height: values[0] })',
    ]

    for (const source of cases) {
      expect(() =>
        transform(`
          import { css } from 'nanocss-compiler'

          const styles = css.create({
            ${source},
          })
        `),
      ).toThrow(
        '[nanocss] css.create(...) dynamic style function parameters must be simple identifiers.',
      )
    }
  })

  it('throws when dynamic style function bodies are not object literals', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: height => {
            return { height }
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.create(...) dynamic style function bodies must be object literals.',
    )
  })

  it('throws when css.create declarations are not top-level', () => {
    const cases = new Map([
      [
        `
        function makeStyles() {
          const styles = css.create({
            root: { color: 'red' },
          })
          return styles
        }
      `,
        '[nanocss] css.create(...) declarations must be at the top level.',
      ],
      [
        `
        {
          const styles = css.create({
            root: { color: 'red' },
          })
        }
      `,
        '[nanocss] css.create(...) declarations must be at the top level.',
      ],
      [
        `
        for (const styles = css.create({ root: { color: 'red' } }); false;) {}
      `,
        '[nanocss] css.create(...) declarations must be at the top level.',
      ],
      [
        `
        function makeVars() {
          const colors = css.defineVars({
            primary: 'red',
          })
          return colors
        }
      `,
        '[nanocss] css.defineVars(...) declarations must be at the top level.',
      ],
      [
        `
        const colors = css.defineVars({
          primary: 'red',
        })
        function makeTheme() {
          const theme = css.createTheme(colors, {
            primary: 'blue',
          })
          return theme
        }
      `,
        '[nanocss] css.createTheme(...) declarations must be at the top level.',
      ],
      [
        `
        function makeKeyframes() {
          const fade = css.keyframes({
            from: { opacity: 0 },
            to: { opacity: 1 },
          })
          return fade
        }
      `,
        '[nanocss] css.keyframes(...) declarations must be at the top level.',
      ],
      [
        `
        function makePositionTry() {
          const fallback = css.positionTry({
            top: '0',
            left: '0',
          })
          return fallback
        }
      `,
        '[nanocss] css.positionTry(...) declarations must be at the top level.',
      ],
      [
        `
        function makeTransition() {
          const transition = css.viewTransitionClass({
            new: { opacity: 1 },
          })
          return transition
        }
      `,
        '[nanocss] css.viewTransitionClass(...) declarations must be at the top level.',
      ],
    ])

    for (const [source, message] of cases) {
      expect(() =>
        transform(`
          import { css } from 'nanocss-compiler'
          ${source}
        `),
      ).toThrow(message)
    }
  })

  it('throws when viewTransitionClass is not assigned to a variable declaration', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        css.viewTransitionClass({
          new: { opacity: 1 },
        })
      `),
    ).toThrow(
      '[nanocss] css.viewTransitionClass(...) must be assigned to a variable declaration.',
    )
  })

  it('throws when positionTry is not assigned to a variable declaration', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        css.positionTry({
          top: '0',
          left: '0',
        })
      `),
    ).toThrow(
      '[nanocss] css.positionTry(...) must be assigned to a variable declaration.',
    )
  })

  it('throws when positionTry uses unsupported static shapes', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const fallback = css.positionTry({
          color: 'red',
        })
      `),
    ).toThrow(
      '[nanocss] css.positionTry(...) only supports positionAnchor, positionArea, inset, margin, size, and self-alignment descriptors.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const fallback = css.positionTry({
          top: getTop(),
        })
      `),
    ).toThrow(
      '[nanocss] css.positionTry(...) style values must be static string, number, boolean, null, variable, constant, keyframes, firstThatWorks, or hook values.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const fallback = css.positionTry({})
      `),
    ).toThrow('[nanocss] css.positionTry(...) must define at least one descriptor.')
  })

  it('throws when viewTransitionClass uses unsupported static shapes', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const transition = css.viewTransitionClass({
          root: { opacity: 1 },
        })
      `),
    ).toThrow(
      '[nanocss] css.viewTransitionClass(...) only supports group, imagePair, old, and new sections.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const transition = css.viewTransitionClass({
          new: {
            opacity: getOpacity(),
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.viewTransitionClass(...) style values must be static string, number, boolean, null, variable, constant, keyframes, firstThatWorks, or hook values.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const transition = css.viewTransitionClass({})
      `),
    ).toThrow(
      '[nanocss] css.viewTransitionClass(...) must define at least one section.',
    )
  })

  it('compiles nested array composition', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          a: {
            marginLeft: 0,
          },
          b: {
            marginRight: 8,
          },
        })

        function Comp() {
          return <div {...css.props([styles.a, [false, null, styles.b], undefined])} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesA = {
          marginLeft: 0
      };
      const _stylesB = {
          marginRight: 8
      };
      function Comp() {
          return <div style={{
              ..._stylesA,
              ..._stylesB
          }}/>;
      }
      "
    `)
  })

  it('reuses direct style expressions for single-item array composition', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        function Comp({ style, fallbackStyle }) {
          return (
            <>
              <div {...css.props([style])} />
              <span {...css.props([[fallbackStyle ?? style]])} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "function Comp({ style, fallbackStyle }) {
          return <>
                    <div style={style}/>
                    <span style={fallbackStyle ?? style}/>
                  </>;
      }
      "
    `)
  })

  it('compiles falsy and logical static style composition', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          disabled: {
            opacity: 0.5,
          },
        })

        function Comp({ isDisabled }) {
          return <div {...css.props(isDisabled && styles.disabled, false, null, undefined)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesDisabled = {
          opacity: 0.5
      };
      function Comp({ isDisabled }) {
          return <div style={{
              ...isDisabled && _stylesDisabled
          }}/>;
      }
      "
    `)
  })

  it('compiles fallback logical style composition', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ style, fallbackStyle }) {
          return (
            <>
              <div {...css.props(style || styles.root)} />
              <span {...css.props(fallbackStyle ?? styles.root)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp({ style, fallbackStyle }) {
          return <>
                    <div style={style || _stylesRoot}/>
                    <span style={fallbackStyle ?? _stylesRoot}/>
                  </>;
      }
      "
    `)
  })

  it('compiles conditional static style composition', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            display: 'flex',
          },
          active: {
            opacity: 1,
          },
          inactive: {
            opacity: 0,
          },
          disabled: {
            pointerEvents: 'none',
          },
        })

        function Comp({ isActive, isDisabled }) {
          return (
            <>
              <div {...css.props(isActive ? styles.active : styles.inactive)} />
              <button {...css.props(styles.root, isDisabled && styles.disabled)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesActive = {
          opacity: 1
      };
      const _stylesInactive = {
          opacity: 0
      };
      const _stylesRoot = {
          display: 'flex'
      };
      const _stylesDisabled = {
          pointerEvents: 'none'
      };
      function Comp({ isActive, isDisabled }) {
          return <>
                    <div style={isActive ? _stylesActive : _stylesInactive}/>
                    <button style={{
              ..._stylesRoot,
              ...isDisabled && _stylesDisabled
          }}/>
                  </>;
      }
      "
    `)
  })

  it('preserves JSX spread ordering around style attributes', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ externalStyle }) {
          return (
            <>
              <div {...css.props(styles.root)} style={externalStyle} />
              <span style={externalStyle} {...css.props(styles.root)} />
              <button id="x" {...css.props(styles.root)} className="foo" />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp({ externalStyle }) {
          return <>
                    <div style={externalStyle}/>
                    <span style={_stylesRoot}/>
                    <button id="x" style={_stylesRoot} className="foo"/>
                  </>;
      }
      "
    `)
  })

  it('preserves JSX ordering across multiple compiled props spreads', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          a: {
            opacity: 1,
          },
          b: {
            opacity: 0.5,
          },
        })

        function Comp() {
          return (
            <>
              <div {...css.props(styles.a)} {...css.props(styles.b)} />
              <span {...css.props(styles.a, styles.b)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesA = {
          opacity: 1
      };
      const _stylesB = {
          opacity: 0.5
      };
      function Comp() {
          return <>
                    <div style={_stylesB}/>
                    <span style={{
              ..._stylesA,
              ..._stylesB
          }}/>
                  </>;
      }
      "
    `)
  })

  it('preserves JSX ordering around unknown prop spreads', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ extraProps }) {
          return (
            <>
              <div {...css.props(styles.root)} {...extraProps} />
              <span {...extraProps} {...css.props(styles.root)} />
            </>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp({ extraProps }) {
          return <>
                    <div style={_stylesRoot} {...extraProps}/>
                    <span {...extraProps} style={_stylesRoot}/>
                  </>;
      }
      "
    `)
  })

  it('preserves create evaluation order for style expressions', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        before()
        const styles = css.create({
          root: {
            color: getColor(),
          },
        })
        after()

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "before();
      const _stylesRoot = {
          color: getColor()
      };
      after();
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('inlines local literal constants in compiled style values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const width = 10
        const color = 'red'
        const styles = css.create({
          root: {
            width,
            color,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          width: 10,
          color: 'red'
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('keeps inlined local literal constants when they are still used', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const width = 10
        const styles = css.create({
          root: {
            width,
          },
        })

        console.log(width)

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const width = 10;
      const _stylesRoot = {
          width: 10
      };
      console.log(width);
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('inlines signed local numeric constants in compiled style values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const marginLeft = -8
        const zIndex = +2
        const styles = css.create({
          root: {
            marginLeft,
            zIndex,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          marginLeft: -8,
          zIndex: 2
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('does not inline local literal constants over dynamic style parameters', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const width = 10
        const styles = css.create({
          root: width => ({
            width,
          }),
        })

        function Comp({ width }) {
          return <div {...css.props(styles.root(width))} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = (width)=>({
              width
          });
      function Comp({ width }) {
          return <div style={_stylesRoot(width)}/>;
      }
      "
    `)
  })

  it('preserves overwritten static style value side effects when merging', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          base: {
            color: before(),
            opacity: 1,
          },
          override: {
            color: 'blue',
            opacity: 0.5,
          },
        })

        function Comp() {
          return <div {...css.props(styles.base, styles.override)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesBase = {
          color: before(),
          opacity: 1
      };
      const _stylesOverride = {
          color: 'blue',
          opacity: 0.5
      };
      function Comp() {
          return <div style={{
              ..._stylesBase,
              ..._stylesOverride
          }}/>;
      }
      "
    `)
  })

  it('preserves props object shape outside JSX spreads', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        const rootProps = css.props(styles.root)
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      const rootProps = {
          style: _stylesRoot
      };
      "
    `)
  })

  it('compiles direct style member references in plain object values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
          outline: {
            outlineOffset: 4,
            outlineStyle: 'solid',
          },
        })

        function getProps() {
          return {
            style: styles.outline,
          }
        }

        function Comp() {
          return <div {...getProps()} {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesOutline = {
          outlineOffset: 4,
          outlineStyle: 'solid'
      };
      const _stylesRoot = {
          opacity: 1
      };
      function getProps() {
          return {
              style: _stylesOutline
          };
      }
      function Comp() {
          return <div {...getProps()} style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('returns empty style props for zero-argument props calls', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const rootProps = css.props()
      `),
    ).toMatchInlineSnapshot(`
      "const rootProps = {
          style: {}
      };
      "
    `)
  })

  it('does not compile shadowed local style group references', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ styles }) {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "function Comp({ styles }) {
          return <div style={styles.root}/>;
      }
      "
    `)
  })

  it('prunes unused local style members from retained groups', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
          unused: {
            color: expensiveColor(),
          },
        })

        console.log(styles.root)
      `),
    ).toMatchInlineSnapshot(`
      "const styles = {
          root: {
              opacity: 1
          }
      };
      console.log(styles.root);
      "
    `)
  })

  it('prunes unused local dynamic style members from retained groups', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
          unused: (width) => ({
            width,
          }),
        })

        console.log(styles.root)
      `),
    ).toMatchInlineSnapshot(`
      "const styles = {
          root: {
              opacity: 1
          }
      };
      console.log(styles.root);
      "
    `)
  })

  it('keeps local style members for dynamic group member access', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
          fallback: {
            color: 'red',
          },
        })

        console.log(styles[getStyleName()])
      `),
    ).toMatchInlineSnapshot(`
      "const styles = {
          root: {
              opacity: 1
          },
          fallback: {
              color: 'red'
          }
      };
      console.log(styles[getStyleName()]);
      "
    `)
  })

  it('does not hoist static merges across local style groups', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const stylesA = css.create({
          a: {
            opacity: 1,
          },
        })

        const width = 10

        const stylesB = css.create({
          b: {
            width,
          },
        })

        function Comp() {
          return <div {...css.props(stylesA.a, stylesB.b)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesAA = {
          opacity: 1
      };
      const _stylesBB = {
          width: 10
      };
      function Comp() {
          return <div style={{
              ..._stylesAA,
              ..._stylesBB
          }}/>;
      }
      "
    `)
  })

  it('compiles dynamic hooked values with property-aware units', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: (width, opacity) => ({
            marginLeft: {
              default: 0,
              ':hover': width,
            },
            opacity: {
              default: 0,
              ':hover': opacity,
            },
          }),
        })

        function Comp({ width, opacity }) {
          return <div {...css.props(styles.root(width, opacity))} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = (width, opacity)=>({
              "--_nanocss_dynamic_onc9m5": typeof width === "number" ? width + "px" : width,
              marginLeft: "var(--_hover-mbscpo-1, var(--_nanocss_dynamic_onc9m5, 0px)) var(--_hover-mbscpo-0, 0px)",
              "--_nanocss_dynamic_onc9n0": typeof opacity === "number" ? opacity : opacity,
              opacity: "var(--_hover-mbscpo-1, var(--_nanocss_dynamic_onc9n0, 0)) var(--_hover-mbscpo-0, 0)"
          });
      function Comp({ width, opacity }) {
          return <div style={_stylesRoot(width, opacity)}/>;
      }
      "
    `)
  })

  it('scopes dynamic hook custom properties by file identity', () => {
    const source = `
      import { css } from 'nanocss-compiler'

      export const styles = css.create({
        root: width => ({
          marginLeft: {
            default: 0,
            ':hover': width,
          },
        }),
      })
    `
    const first = transform(source, { filename: 'src/a.tsx' })!
    const second = transform(source, { filename: 'src/b.tsx' })!
    const firstName = first.match(/"--_nanocss_dynamic_[^"]+"/)?.[0]
    const secondName = second.match(/"--_nanocss_dynamic_[^"]+"/)?.[0]

    expect(firstName).toBeTruthy()
    expect(secondName).toBeTruthy()
    expect(firstName).not.toBe(secondName)
  })

  it('scopes generated defineVars custom properties by file identity', () => {
    const source = `
      import { css } from 'nanocss-compiler'

      export const colors = css.defineVars({
        primary: 'green',
      })
    `
    const first = transform(source, { filename: 'src/a.css.ts' })!
    const second = transform(source, { filename: 'src/b.css.ts' })!
    const firstName = first.match(/"--_nanocss_var_[^"]+"/)?.[0]
    const secondName = second.match(/"--_nanocss_var_[^"]+"/)?.[0]

    expect(firstName).toBeTruthy()
    expect(secondName).toBeTruthy()
    expect(firstName).not.toBe(secondName)
  })

  it('compiles dynamic hooked values inside nested conditions', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: (color) => ({
            color: {
              default: 'black',
              ':hover': {
                default: 'red',
                '@media (min-width: 768px)': color,
              },
            },
          }),
        })

        function Comp({ color }) {
          return <div {...css.props(styles.root(color))} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = (color)=>({
              "--_nanocss_dynamic_onc9m5": typeof color === "number" ? color + "px" : color,
              color: "var(--cond-27myt-1, var(--_nanocss_dynamic_onc9m5, var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black))) var(--cond-27myt-0, var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black))"
          });
      function Comp({ color }) {
          return <div style={_stylesRoot(color)}/>;
      }
      "
    `)
  })

  it('throws when compiled style objects are referenced directly', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp() {
          return <div style={styles.root} />
        }
      `),
    ).toThrow(
      '[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...).',
    )
  })

  it('throws when compiled style objects are referenced inside nested JSX style values', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp() {
          return <div style={{
            get value() {
              return styles.root
            }
          }} />
        }
      `),
    ).toThrow(
      '[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...).',
    )
  })

  it('throws when a style uses a shorthand property', () => {
    for (const property of [
      'animationRange',
      'background',
      'backgroundPosition',
      'borderLeft',
      'caret',
      'container',
      'cornerShape',
      'gap',
      'marker',
      'maskBorder',
      'paddingInline',
      'scrollTimeline',
      'stroke',
      'textEmphasis',
      'textWrap',
    ]) {
      expect(() =>
        transform(`
          import { css } from 'nanocss-compiler'

          const styles = css.create({
            root: {
              ${property}: 0,
            },
          })
        `),
      ).toThrow(`CSS shorthand property "${property}"`)
    }

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            margin: 0,
          },
        })
      `),
    ).toThrow(
      '[nanocss] CSS shorthand property "margin" is not supported by the compiler. Use longhand properties instead.',
    )
  })

  it('throws when hook objects omit default', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              ':hover': 'red',
            },
          },
        })
      `),
    ).toThrow('[nanocss] Hook objects must include a default value.')

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':hover': {
                '@media (min-width: 768px)': 'red',
              },
            },
          },
        })
      `),
    ).toThrow('[nanocss] Hook objects must include a default value.')
  })

  it('throws when firstThatWorks receives non-static arguments', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            position: css.firstThatWorks(getPosition(), 'fixed'),
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.firstThatWorks(...) must be called with static string or number arguments.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            position: css.firstThatWorks(...positions),
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.firstThatWorks(...) must be called with static string or number arguments.',
    )
  })

  it('throws when defineVars uses numeric defaults', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const vars = css.defineVars({
          space: 4,
        })
      `),
    ).toThrow(
      '[nanocss] css.defineVars(...) numeric defaults are not supported. Use strings such as "4px" or "0.5" instead.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const vars = css.defineVars({
          space: {
            default: '0px',
            ':hover': 4,
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.defineVars(...) numeric defaults are not supported. Use strings such as "4px" or "0.5" instead.',
    )
  })

  it('throws when defineVars uses the reserved defaults key', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const vars = css.defineVars({
          $$defaults: 'green',
        })
      `),
    ).toThrow(
      '[nanocss] css.defineVars(...) token names cannot use the reserved "$$defaults" key.',
    )
  })

  it('throws when defineVars stores a generated view transition class', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const pageTransition = css.viewTransitionClass({
          new: { opacity: 1 },
        })

        const vars = css.defineVars({
          pageTransition,
        })
      `),
    ).toThrow(
      '[nanocss] css.defineVars(...) can only store generated css.keyframes(...) or css.positionTry(...) strings.',
    )
  })

  it('throws when keyframes or positionTry declarations are exported directly', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        export const fade = css.keyframes({
          from: { opacity: 0 },
          to: { opacity: 1 },
        })
      `),
    ).toThrow(
      '[nanocss] Exported css.keyframes(...) declarations must be wrapped in css.defineVars(...) for cross-file style use.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        export const fallback = css.positionTry({
          top: '0',
        })
      `),
    ).toThrow(
      '[nanocss] Exported css.positionTry(...) declarations must be wrapped in css.defineVars(...) for cross-file style use.',
    )
  })

  it('throws when createTheme uses numeric overrides', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const vars = css.defineVars({
          space: '4px',
        })

        const theme = css.createTheme(vars, {
          space: 8,
        })
      `),
    ).toThrow(
      '[nanocss] css.createTheme(...) numeric overrides are not supported. Use strings such as "4px" or "0.5" instead.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const vars = css.defineVars({
          space: '4px',
        })

        const theme = css.createTheme(vars, {
          space: {
            default: '4px',
            ':hover': 8,
          },
        })
      `),
    ).toThrow(
      '[nanocss] css.createTheme(...) numeric overrides are not supported. Use strings such as "4px" or "0.5" instead.',
    )
  })

  it('throws when exported defineVars or createTheme are outside css module files', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        export const colors = css.defineVars({
          primary: 'green',
        })
      `),
    ).toThrow(
      '[nanocss] Exported css.defineVars(...) declarations must be in *.css.ts files.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        export const theme = css.createTheme(colors, {
          primary: 'purple',
        })
      `),
    ).toThrow(
      '[nanocss] Exported css.createTheme(...) declarations must be in *.css.ts files.',
    )
  })

  it('throws when exported defineConsts is outside css module files', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        export const tokens = css.defineConsts({
          compact: '@media (max-width: 700px)',
        })
      `),
    ).toThrow(
      '[nanocss] Exported css.defineConsts(...) declarations must be in *.css.ts files.',
    )
  })

  it('throws when defineConsts is not exported from css module files', () => {
    expect(() =>
      transform(
        `
          import { css } from 'nanocss-compiler'

          const tokens = css.defineConsts({
            compact: '@media (max-width: 700px)',
          })
        `,
        { filename: 'src/tokens.css.ts' },
      ),
    ).toThrow(
      '[nanocss] css.defineConsts(...) declarations must be exported from *.css.ts files.',
    )
  })

  it('throws when defineConsts is nested', () => {
    expect(() =>
      transform(
        `
          import { css } from 'nanocss-compiler'

          export function makeTokens() {
            const tokens = css.defineConsts({
              compact: '@media (max-width: 700px)',
            })
            return tokens
          }
        `,
        { filename: 'src/tokens.css.ts' },
      ),
    ).toThrow(
      '[nanocss] css.defineConsts(...) declarations must be at the top level.',
    )
  })

  it('throws when createTheme is not assigned to a variable declaration', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const props = {
          style: css.createTheme(colors, {
            primary: 'purple',
          }),
        }
      `),
    ).toThrow(
      '[nanocss] css.createTheme(...) must be assigned to a variable declaration.',
    )
  })

  it('throws when hook objects use unsupported condition keys', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              '&[data-disabled]': 'red',
            },
          },
        })
      `),
    ).toThrow(
      '[nanocss] "&[data-disabled]" is not a valid hook name. Hooks must be default, or start with "@", ":", or "[".',
    )
  })

  it('throws for unsupported create shapes', () => {
    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
          ...otherStyles,
        })
      `),
    ).toThrow('[nanocss] css.create(...) objects cannot contain spreads.')

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({})
      `),
    ).toThrow('[nanocss] css.create(...) must define at least one style.')

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create(getStyles())
      `),
    ).toThrow(
      '[nanocss] css.create(...) must be called with a static object expression.',
    )

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const key = getKey()
        const styles = css.create({
          root: {
            [key]: 'red',
          },
        })
      `),
    ).toThrow('[nanocss] Style property keys must be statically known.')

    expect(() =>
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        }), other = after()
      `),
    ).toThrow(
      '[nanocss] css.create(...) declarations must not share a variable declaration with other declarators.',
    )
  })

  it('throws instead of silently ignoring invalid SWC plugin options JSON', () => {
    expect(() =>
      transformSwcSync(
        `
          import { css } from 'nanocss-compiler'

          const styles = css.create({
            root: {
              opacity: 1,
            },
          })
        `,
        {
          filename: 'test.tsx',
          jsc: {
            parser: {
              syntax: 'typescript',
              tsx: true,
            },
            experimental: {
              plugins: [[path.join(repoRoot, 'dist/swc.wasm'), 'not json' as never]],
            },
          },
          module: {
            type: 'es6',
          },
        },
      ),
    ).toThrow('failed to invoke plugin')
  })

  it('supports custom import sources', () => {
    expect(
      transform(
        `
          import { css } from '@/lib/nanocss'

          const styles = css.create({
            root: {
              opacity: 1,
            },
          })

          function Comp() {
            return <div {...css.props(styles.root)} />
          }
        `,
        {
          importSources: ['@/lib/nanocss'],
        },
      ),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('supports import aliases and respects shadowed bindings', () => {
    expect(
      transform(`
        import { css as x } from 'nanocss-compiler'

        const styles = x.create({
          root: {
            opacity: 1,
          },
        })

        function Comp() {
          return <div {...x.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)

    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        const styles = {
          root: {
            opacity: 1,
          },
        }

        function Comp() {
          const css = otherCss
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const styles = {
          root: {
              opacity: 1
          }
      };
      function Comp() {
          const css = otherCss;
          return <div {...css.props(styles.root)}/>;
      }
      export { };
      "
    `)
  })

  it('compiles html elements to native JSX with style composition', () => {
    expect(
      transform(`
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          foo: {
            opacity: 1,
          },
          bar: {
            display: 'flex',
          },
        })

        function Comp() {
          return <html.div id="x" style={[styles.foo, styles.bar]} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesFoo = {
          opacity: 1
      };
      const _stylesBar = {
          display: 'flex'
      };
      function Comp() {
          return <div data-element-src="test.tsx:14" id="x" style={{
              ..._stylesFoo,
              ..._stylesBar
          }}/>;
      }
      "
    `)
  })

  it('compiles html element aliases and closing tags', () => {
    expect(
      transform(`
        import { css, html as h } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            paddingLeft: 8,
          },
          child: {
            color: 'red',
          },
        })

        function Comp() {
          return (
            <h.section style={styles.root}>
              <h.span style={[styles.child]}>Hello</h.span>
            </h.section>
          )
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          paddingLeft: 8
      };
      const _stylesChild = {
          color: 'red'
      };
      function Comp() {
          return <section data-element-src="test.tsx:15" style={_stylesRoot}>
                    <span data-element-src="test.tsx:16" style={_stylesChild}>Hello</span>
                  </section>;
      }
      "
    `)
  })

  it('compiles html dynamic style composition', () => {
    expect(
      transform(`
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ style }) {
          return <html.div style={[style, styles.root]} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp({ style }) {
          return <div data-element-src="test.tsx:11" style={{
              ...style,
              ..._stylesRoot
          }}/>;
      }
      "
    `)
  })

  it('does not add html default styles by default', () => {
    expect(
      transform(
        `
        import { html } from 'nanocss-compiler'

        function Comp() {
          return (
            <>
              <html.div />
              <html.span />
              <html.br />
            </>
          )
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "function Comp() {
          return <>
                    <div/>
                    <span/>
                    <br/>
                  </>;
      }
      "
    `)
  })

  it('does not wrap a single html style reference without defaults', () => {
    expect(
      transform(
        `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          logoRow: {
            display: 'flex',
            alignItems: 'center',
          },
        })

        function Comp() {
          return <html.div style={styles.logoRow} />
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _styles = {
          display: 'flex',
          alignItems: 'center'
      };
      function Comp() {
          return <div style={_styles}/>;
      }
      "
    `)
  })

  it('uses configured html default styles', () => {
    expect(
      transform(
        `
        import { html } from 'nanocss-compiler'

        function Comp() {
          return (
            <>
              <html.div />
              <html.span />
            </>
          )
        }
      `,
        {
          debug: false,
          htmlDefaults: {
            div: {
              boxSizing: 'border-box',
              marginTop: 0,
            },
          },
        },
      ),
    ).toMatchInlineSnapshot(`
      "const _htmlDivDefaultStyle = {
          boxSizing: "border-box",
          marginTop: 0
      };
      function Comp() {
          return <>
                    <div style={_htmlDivDefaultStyle}/>
                    <span/>
                  </>;
      }
      "
    `)
  })

  it('preserves html default style order around JSX spreads', () => {
    expect(
      transform(
        `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ extraProps }) {
          return (
            <>
              <html.div {...extraProps} />
              <html.div {...extraProps} style={styles.root} />
            </>
          )
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _styles = {
          opacity: 1
      };
      function Comp({ extraProps }) {
          return <>
                    <div {...extraProps} style={extraProps?.style}/>
                    <div {...extraProps} style={{
              ...extraProps?.style,
              ..._styles
          }}/>
                  </>;
      }
      "
    `)
  })

  it('evaluates html spread props once when reading spread styles', () => {
    expect(
      transform(
        `
        import { html } from 'nanocss-compiler'

        function Comp() {
          return <html.div {...getProps()} id={second()} {...third()} />
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "function Comp() {
          return (()=>{
              let _htmlProps, _htmlProps2;
              return <div {..._htmlProps = getProps()} id={second()} {..._htmlProps2 = third()} style={{
                  ..._htmlProps?.style,
                  ..._htmlProps2?.style
              }}/>;
          })();
      }
      "
    `)
  })

  it('generates html spread props temps without colliding with user bindings', () => {
    expect(
      transform(
        `
        import { html } from 'nanocss-compiler'

        function Comp(_htmlProps) {
          const _htmlProps2 = {}
          return <html.div {...getProps()} />
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "function Comp(_htmlProps) {
          const _htmlProps2 = {};
          return (()=>{
              let _htmlProps3;
              return <div {..._htmlProps3 = getProps()} style={_htmlProps3?.style}/>;
          })();
      }
      "
    `)
  })

  it('localizes html spread props temps to nested JSX children', () => {
    expect(
      transform(
        `
        import { html } from 'nanocss-compiler'

        function Comp() {
          return <html.main><html.a {...getProps()} /></html.main>
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "function Comp() {
          return <main>{(()=>{
              let _htmlProps;
              return <a {..._htmlProps = getProps()} style={_htmlProps?.style}/>;
          })()}</main>;
      }
      "
    `)
  })

  it('lets later html spreads override earlier explicit styles', () => {
    expect(
      transform(
        `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ extraProps }) {
          return <html.div style={styles.root} {...extraProps} />
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _styles = {
          opacity: 1
      };
      function Comp({ extraProps }) {
          return <div {...extraProps} style={{
              ..._styles,
              ...extraProps?.style
          }}/>;
      }
      "
    `)
  })

  it('compiles html conditional style composition', () => {
    expect(
      transform(
        `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          a: {
            opacity: 1,
          },
          b: {
            opacity: 0.5,
          },
          c: {
            display: 'flex',
          },
        })

        function Comp({ condition }) {
          return (
            <>
              <html.div style={[styles.a, condition && styles.b]} />
              <html.span style={condition ? styles.b : styles.c} />
            </>
          )
        }
      `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _styles = {
          opacity: 1
      };
      const _styles2 = {
          opacity: 0.5
      };
      const _styles3 = {
          display: 'flex'
      };
      function Comp({ condition }) {
          return <>
                    <div style={{
              ..._styles,
              ...condition && _styles2
          }}/>
                    <span style={condition ? _styles2 : _styles3}/>
                  </>;
      }
      "
    `)
  })

  it('does not compile shadowed html elements', () => {
    expect(
      transform(`
        import { html } from 'nanocss-compiler'

        function Comp() {
          const html = otherHtml
          return <html.div />
        }
      `),
    ).toMatchInlineSnapshot(`
      "function Comp() {
          const html = otherHtml;
          return <html.div/>;
      }
      export { };
      "
    `)
  })

  it('omits html element source attributes outside debug mode', () => {
    expect(
      transform(
        `
          import { css, html } from 'nanocss-compiler'

          const styles = css.create({
            root: {
              opacity: 1,
            },
          })

          function Comp() {
            return <html.div style={styles.root} />
          }
        `,
        { debug: false },
      ),
    ).toMatchInlineSnapshot(`
      "const _styles = {
          opacity: 1
      };
      function Comp() {
          return <div style={_styles}/>;
      }
      "
    `)
  })

  it('uses source-mapped line numbers for html element source attributes', () => {
    const result = transformWithMetadata(
      `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp() {
          return <html.div style={styles.root} />
        }
      `,
      {
        filename: 'generated.tsx',
        inputSourceMap: {
          version: 3,
          file: 'generated.tsx',
          names: [],
          sources: ['original.tsx'],
          sourcesContent: [null],
          mappings: ';;;;;;;;;;AAyCA',
        },
      },
    )

    expect(result?.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1
      };
      function Comp() {
          return <div data-element-src="generated.tsx:42" style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('compiles exported styles to plain style objects', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'

        export const styles = css.create({
          root: {
            opacity: 1,
          },
          unused: {
            color: 'red',
          },
          dynamic: (width) => ({
            width,
          }),
        })
      `),
    ).toMatchInlineSnapshot(`
      "export const styles = {
          root: {
              opacity: 1
          },
          unused: {
              color: 'red'
          },
          dynamic: (width)=>({
                  width
              })
      };
      "
    `)
  })

  it('treats imported styles as plain style objects', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { styles as importedStyles } from './styles'

        const styles = css.create({
          root: {
            opacity: 1,
          },
        })

        function Comp({ width }) {
          return <div {...css.props(styles.root, importedStyles.b, importedStyles.dynamic(width))} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { styles as importedStyles } from './styles';
      const _stylesRoot = {
          opacity: 1
      };
      function Comp({ width }) {
          return <div style={{
              ..._stylesRoot,
              ...importedStyles.b,
              ...importedStyles.dynamic(width)
          }}/>;
      }
      "
    `)
  })

  it('wraps imported nested variable tokens as CSS variable values', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { colors } from './colors.css'

        const styles = css.create({
          root: {
            borderTopColor: colors.nested,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { colors } from './colors.css';
      const _stylesRoot = {
          borderTopColor: "var(" + colors.nested + ", var(" + (colors.nested + "--n-default") + "))"
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })

  it('does not wrap imported member values from non-css modules', () => {
    expect(
      transform(`
        import { css } from 'nanocss-compiler'
        import { tokens } from './tokens'

        const styles = css.create({
          root: {
            color: tokens.primary,
          },
        })

        function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `),
    ).toMatchInlineSnapshot(`
      "import { tokens } from './tokens';
      const _stylesRoot = {
          color: tokens.primary
      };
      function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
  })
})
