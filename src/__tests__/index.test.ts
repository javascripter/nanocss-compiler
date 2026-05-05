import { it, expect } from 'vitest'
import { css, html } from '../index'

declare module '../index' {
  namespace css {
    interface Register {
      env: {
        colors: {
          primary: string
        }
      }
    }
  }
}

if (false) {
  const register: css.Register = {} as css.Register
  const envPrimary: string = css.env.colors.primary
  const styleProp: css.StyleProp = [{ color: 'red' }, false]
  expect(register).toBeTypeOf('object')
  expect(envPrimary).toBeTypeOf('string')
  expect(styleProp).toBeTypeOf('object')
}

it('should not expose styleSheet at runtime', () => {
  expect(css).not.toHaveProperty('styleSheet')
})

it('should throw when create is called at runtime', () => {
  expect(() =>
    css.create({
      root: {
        opacity: 1,
      },
    }),
  ).toThrow(
    '[nanocss] css.create(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when props is called at runtime', () => {
  expect(() => css.props({} as never)).toThrow(
    '[nanocss] css.props(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when defineVars is called at runtime', () => {
  expect(() =>
    css.defineVars({
      primary: 'green',
    }),
  ).toThrow(
    '[nanocss] css.defineVars(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when defineConsts is called at runtime', () => {
  expect(() =>
    css.defineConsts({
      compact: '@media (max-width: 700px)',
    }),
  ).toThrow(
    '[nanocss] css.defineConsts(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when createTheme is called at runtime', () => {
  expect(() =>
    css.createTheme(
      {
        primary: 'var(--_nanocss_var_0, green)',
      },
      {
        primary: 'red',
      },
    ),
  ).toThrow(
    '[nanocss] css.createTheme(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when keyframes is called at runtime', () => {
  expect(() =>
    css.keyframes({
      '0%': { opacity: 0 },
      '100%': { opacity: 1 },
    }),
  ).toThrow(
    '[nanocss] css.keyframes(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when positionTry is called at runtime', () => {
  expect(() =>
    css.positionTry({
      top: '0',
      left: '0',
      width: '100px',
      height: '100px',
    }),
  ).toThrow(
    '[nanocss] css.positionTry(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when viewTransitionClass is called at runtime', () => {
  expect(() =>
    css.viewTransitionClass({
      new: {
        animationDuration: '200ms',
      },
    }),
  ).toThrow(
    '[nanocss] css.viewTransitionClass(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when firstThatWorks is called at runtime', () => {
  expect(() => css.firstThatWorks('sticky', 'fixed')).toThrow(
    '[nanocss] css.firstThatWorks(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should throw when types are called at runtime', () => {
  expect(() => css.types.color('blue')).toThrow(
    '[nanocss] css.types.*(...) is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})

it('should expose an empty env object at runtime', () => {
  expect(css.env).toEqual({})
})

it('should throw when html elements are used at runtime', () => {
  expect(() => html.div).toThrow(
    '[nanocss] html.div is a compile-time API. Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.',
  )
})
