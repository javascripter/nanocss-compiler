import { describe, expect, it } from 'vitest'
import { transformSync } from '../native'

describe('native transform wrapper', () => {
  it('requires an options object', () => {
    expect(() => {
      transformSync('', undefined as never)
    }).toThrow('[nanocss] transformSync(...) options are required.')
  })

  it('requires options.filename', () => {
    expect(() => {
      transformSync('', {} as never)
    }).toThrow('[nanocss] transformSync(...) options.filename is required.')
  })

  it('transforms code and returns stylesheet metadata', () => {
    const result = transformSync(
      `
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            opacity: 1,
            color: { default: 'black', ':hover': 'red' },
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
      { filename: 'src/app.tsx', debug: true },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "const _stylesRoot = {
          opacity: 1,
          color: "var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)"
      };
      export function Comp() {
          return <div style={_stylesRoot}/>;
      }
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toMatchInlineSnapshot(`
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

  it('returns metadata without code when code is false', () => {
    const result = transformSync(
      `
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: { default: 'black', ':hover': 'red' },
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
      { filename: 'src/app.tsx', debug: true, code: false },
    )

    expect(result.code).toBe(null)
    expect(result.metadata.nanocss.styleSheet).toContain('*:hover')
  })

  it('extracts variables, themes, and keyframes metadata', () => {
    const result = transformSync(
      `
        import { css } from 'nanocss-compiler'

        export const colors = css.defineVars({
          primary: 'green',
          accent: { default: 'black', ':hover': 'red' },
        })

        export const theme = css.createTheme(colors, {
          primary: 'purple',
        })

        const fade = css.keyframes({
          from: { opacity: 0 },
          to: { opacity: 1 },
        })

        export const animations = css.defineVars({
          fade,
        })
      `,
      { filename: 'src/tokens.css.ts', debug: true },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "export const colors = {
          "primary": "--_nanocss_var_colors_primary_vec0x7",
          "accent": "--_nanocss_var_colors_accent_6a3xtt",
          "$$defaults": {
              "--_nanocss_var_colors_primary_vec0x7": "var(--_nanocss_var_colors_primary_vec0x7--n-default)",
              "--_nanocss_var_colors_accent_6a3xtt": "var(--_nanocss_var_colors_accent_6a3xtt--n-default)"
          }
      };
      export const theme = {
          ...colors.$$defaults,
          "--_nanocss_var_colors_primary_vec0x7": 'purple'
      };
      const fade = "__nanocss_keyframes-firn26";
      export const animations = {
          "fade": "--_nanocss_var_animations_fade_mvxxec",
          "$$defaults": {
              "--_nanocss_var_animations_fade_mvxxec": "var(--_nanocss_var_animations_fade_mvxxec--n-default)"
          }
      };
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toMatchInlineSnapshot(`
      "* {
        --_hover-mbscpo-0: initial;
        --_hover-mbscpo-1: ;
      }
      *:hover {
        --_hover-mbscpo-0: ;
        --_hover-mbscpo-1: initial;
      }

      @keyframes __nanocss_keyframes-firn26 {
        from {
          opacity: 0;
        }
        to {
          opacity: 1;
        }
      }
      * {
        --_nanocss_var_colors_primary_vec0x7--n-default: green;
        --_nanocss_var_colors_accent_6a3xtt--n-default: var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black);
        --_nanocss_var_animations_fade_mvxxec--n-default: __nanocss_keyframes-firn26;
      }"
    `)
  })

  it('lowers html elements with default styles', () => {
    const result = transformSync(
      `
        import { css, html } from 'nanocss-compiler'

        const styles = css.create({
          a: { color: 'red' },
          b: { opacity: 1 },
        })

        export function Comp({ extraProps }) {
          return <html.div {...extraProps} style={[styles.a, styles.b]} />
        }
      `,
      {
        filename: 'src/app.tsx',
        debug: true,
        htmlDefaults: {
          div: {
            boxSizing: 'border-box',
          },
        },
      },
    )

    expect(result.code).toMatchInlineSnapshot(`
      "const _htmlDivDefaultStyle = {
          boxSizing: "border-box"
      };
      const _stylesA = {
          color: 'red'
      };
      const _stylesB = {
          opacity: 1
      };
      export function Comp({ extraProps }) {
          return <div data-element-src="src/app.tsx:10" {...extraProps} style={{
              ..._htmlDivDefaultStyle,
              ...extraProps?.style,
              ..._stylesA,
              ..._stylesB
          }}/>;
      }
      "
    `)
    expect(result.metadata.nanocss.styleSheet).toBe('')
  })
})
