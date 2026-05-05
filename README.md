# NanoCSS Compiler

> Experimental: This is a PoC and not published on npm.

NanoCSS Compiler is a StyleX-like inline style authoring API that is designed to be
compiled away. You write `css.create()`, `css.props()`, `css.defineVars()`,
`css.keyframes()`, `css.positionTry()`, `css.viewTransitionClass()`,
`css.types.*()`, and `html.*` JSX in source, and the NanoCSS Compiler turns
them into plain React props plus extracted CSS.

The runtime exports `css` and `html` authoring objects. `css.create()`,
`css.props()`, `css.defineVars()`, `css.createTheme()`, `css.defineConsts()`,
`css.keyframes()`, `css.positionTry()`, `css.firstThatWorks()`,
`css.viewTransitionClass()`, `css.types.*()`, `css.env.*`, and `html.*` JSX are
compile-time APIs. Function APIs throw if they are used at runtime; `css.env`
is an empty frozen object at runtime and is meant to be replaced by the
compiler.

## Features

- StyleX-like `css.create()` and `css.props()` API.
- Optional `html.*` JSX API for `style={[styles.a, styles.b]}` composition.
- Compiles local static styles to hoisted plain style objects.
- Compiles exported `css.create()` calls to plain style object exports.
- Compiles `css.props()` JSX spreads to `style={...}` while preserving JSX prop
  overwrite order.
- Supports dynamic style functions such as `styles.root(width)`.
- Supports nested hooks such as `:hover`, `:has(...)`, `[data-state]`,
  `@media`, `@container`, and `@supports`.
- Supports `defineVars`, resettable `createTheme`, `defineConsts`,
  `keyframes`, `positionTry`, `firstThatWorks`, `viewTransitionClass`, and
  typed variables through `css.types.*`.
- Supports JSON compile-time configuration reads through `css.env`.
- Records generated hook, variable, keyframe, `@position-try`, and view
  transition CSS in compiler metadata.
- Rejects CSS shorthands in compiled styles so object spread semantics stay
  predictable.

## Installation

```bash
npm install nanocss-compiler
```

## Setup

Import `css` from NanoCSS Compiler in files that author styles. Import `html` when using
the JSX element API.

```ts
import { css, html } from 'nanocss-compiler'
```

Configure one compiler for code lowering and the PostCSS plugin for stylesheet
extraction. The compiler turns NanoCSS Compiler authoring APIs into plain React style
objects. The PostCSS plugin scans source files with the native extractor and
replaces `@nanocss` with generated hook, variable, keyframe, `@position-try`,
and view transition CSS.

Use the same `debug` value for the code compiler and the PostCSS plugin. The
generated names are part of the compiled ABI.

Configure `importSources` only if your app re-exports `css` from a project-local
module.

The default `importSources` contain `nanocss-compiler`.

Pass the same `env` object to the code compiler and PostCSS plugin when using
`css.env`. `env` must be JSON-serializable.

NanoCSS Compiler ships an SWC transform as a wasm plugin. It compiles the authoring APIs
and is the preferred setup for Next.js Turbopack.

```js
// next.config.mjs
export default {
  experimental: {
    swcPlugins: [
      [
        'nanocss-compiler/swc',
        {
          debug: process.env.NODE_ENV !== 'production',
          env: {
            compact: '@media (max-width: 700px)',
          },
        },
      ],
    ],
  },
}
```

Add `@nanocss` to a global CSS file and configure the PostCSS plugin:

```css
@nanocss;
```

```js
// postcss.config.cjs
module.exports = {
  plugins: {
    'nanocss-compiler/postcss-plugin': {
      include: ['src/**/*.{js,jsx,mjs,ts,tsx}'],
      debug: process.env.NODE_ENV !== 'production',
      env: {
        compact: '@media (max-width: 700px)',
      },
      // Optional, for html.* intrinsic element defaults.
      htmlDefaults: {
        div: {
          boxSizing: 'border-box',
        },
      },
    },
  },
}
```

## Basic Usage

```tsx
import { css } from 'nanocss-compiler'

const styles = css.create({
  root: {
    display: 'flex',
    paddingTop: 16,
    paddingRight: 16,
    paddingBottom: 16,
    paddingLeft: 16,
  },
  title: {
    color: {
      default: 'black',
      ':hover': 'blue',
    },
  },
})

export function App() {
  return <h1 {...css.props(styles.root, styles.title)}>Hello</h1>
}
```

The compiler turns the component path into plain style output:

```tsx
const _styles = {
  display: 'flex',
  paddingTop: 16,
  paddingRight: 16,
  paddingBottom: 16,
  paddingLeft: 16,
  color: 'var(--_hover-mbscpo-1, blue) var(--_hover-mbscpo-0, black)',
}

export function App() {
  return <h1 style={_styles}>Hello</h1>
}
```

The exact generated custom property names depend on the inferred hook conditions
and the compiler `debug` option.

## Compiler Contract

`css.create()`, `css.props()`, and `html.*` JSX must be compiled away. Runtime
usage throws with an error that points at the missing compiler setup.
`css.defineVars()`, `css.createTheme()`, `css.defineConsts()`,
`css.keyframes()`, `css.positionTry()`, `css.firstThatWorks()`, and
`css.viewTransitionClass()` follow the same rule.

### `css.create()` must be static

The compiler only supports static top-level `css.create()` calls.

```ts
const styles = css.create({
  root: {
    opacity: 1,
  },
})
```

These forms are rejected:

```ts
const styles = css.create(getStyles())

const styles = css.create({
  root: {
    opacity: 1,
  },
  ...otherStyles,
})

const styles = css.create({
  [name]: {
    opacity: 1,
  },
})
```

Each style entry must be either an object expression or an arrow function that
returns an object expression. Dynamic function arguments must be simple
identifiers. Destructuring, default values, rest parameters, function
expressions, and block bodies are rejected.

```ts
const styles = css.create({
  root: {
    opacity: 1,
  },
  sized: (width: number) => ({
    width,
  }),
})
```

`css.defineVars()`, `css.createTheme()`, `css.defineConsts()`, `css.keyframes()`,
`css.positionTry()`, and `css.viewTransitionClass()` also need top-level
variable declarations. `css.defineConsts()` must be exported from `*.css.ts`
files.

### Styles must use longhand properties

Compiled style objects cannot use CSS shorthands such as `margin`,
`padding`, `background`, `border`, `gap`, `font`, `transition`, or `animation`.
Use longhand properties instead:

```ts
const styles = css.create({
  root: {
    marginTop: 8,
    marginRight: 8,
    marginBottom: 8,
    marginLeft: 8,
  },
})
```

This restriction is what lets NanoCSS Compiler compile style composition to plain object
spread semantics without preserving the old `delete` and re-add ordering logic.

### Hook objects require `default`

Every hook object must include a `default` value.

Supported hook keys are `default`, pseudo selectors starting with `:`,
attribute selectors starting with `[`, and at-rules starting with `@`.
Examples include `:hover`, `:focus-visible`, `:is([data-disabled])`,
`:has(> img)`, `[data-state="open"]`, `@media (...)`, `@container ...`,
`@supports (...)`, and `@starting-style`. Selector hooks cannot use `&`;
write `:hover` or `[data-disabled]`, not `&:hover` or `&[data-disabled]`.

```ts
const styles = css.create({
  root: {
    color: {
      default: 'black',
      ':hover': 'red',
    },
  },
})
```

Nested hook objects also require `default`:

```ts
const styles = css.create({
  root: {
    color: {
      default: 'black',
      ':hover': {
        default: 'red',
        '@media (min-width: 768px)': 'blue',
      },
    },
  },
})
```

### Dynamic style functions

Dynamic style functions are compiled to plain functions that return style
objects.

```tsx
const styles = css.create({
  root: (width: number, opacity: number) => ({
    width,
    opacity,
    marginLeft: {
      default: 0,
      ':hover': width,
    },
  }),
})

function Component({ width }: { width: number }) {
  return <div {...css.props(styles.root(width, 0.8))} />
}
```

For numeric dynamic hook values, NanoCSS Compiler serializes values according to the CSS
property. Length-like properties get `px`; unitless properties do not.

### Style composition

Use `css.props()` to compose styles.

```tsx
<div {...css.props(styles.base, styles.variant, isActive && styles.active)} />
```

Or use the `html.*` JSX API when you only need style composition:

```tsx
<html.div style={[styles.base, styles.variant, isActive && styles.active]} />
```

`html.*` elements lower to regular intrinsic JSX elements. They support
`style={[styles.a, styles.b]}` composition and merge `style` values from prop
spreads using optional access, so nullable prop spreads do not throw while
reading `.style`.

`html.*` has no default reset styles unless `htmlDefaults` is configured. When a
default is configured for a tag, user styles and `style` values from prop
spreads are merged after the default style so user styles win.
When `debug` is enabled, lowered elements also get a `data-element-src`
attribute with the source file and line number.

Later arguments win over earlier arguments. Arrays, nested arrays, falsy values,
conditional expressions, and logical expressions are supported where they can be
compiled statically:

```tsx
<div {...css.props([styles.base, [false, null, styles.variant], undefined])} />
<div {...css.props(isActive ? styles.active : styles.inactive)} />
<div {...css.props(styles.root, isDisabled && styles.disabled)} />
```

Multiple JSX spreads preserve JSX overwrite semantics:

```tsx
<div {...css.props(styles.a)} {...css.props(styles.b)} />
```

is different from:

```tsx
<div {...css.props(styles.a, styles.b)} />
```

The first form means the second spread replaces the whole `style` prop. The
second form means NanoCSS Compiler merges both style objects.

### Exported styles

Exported `css.create()` calls compile to plain style objects so compiled files can
be imported without a NanoCSS Compiler runtime dependency for those styles.

```ts
export const styles = css.create({
  root: {
    opacity: 1,
  },
  dynamic: (width: number) => ({
    width,
  }),
})
```

compiles to a shape equivalent to:

```ts
export const styles = {
  root: {
    opacity: 1,
  },
  dynamic: (width: number) => ({
    width,
  }),
}
```

Usage sites can compose local and imported compiled styles:

```tsx
<div
  {...css.props(styles.root, importedStyles.card, importedStyles.dynamic(width))}
/>
```

## Variables and Themes

NanoCSS Compiler supports compile-time `css.defineVars()` and `css.createTheme()` for CSS
variables and theme overrides. Static `css.defineVars()` declarations compile to
plain objects whose values are custom property names. They also include an
internal `$$defaults` map that lets themes reset omitted tokens:

```ts
{
  primary: '--_nanocss_var_hbd9hf',
  $$defaults: {
    '--_nanocss_var_hbd9hf': 'var(--_nanocss_var_hbd9hf--n-default)'
  }
}
```

Local `css.createTheme()` calls accept partial overrides and compile to style
objects that first spread `vars.$$defaults`, then apply explicit overrides. This
means `css.createTheme(vars, {})` is a reset theme, and merging a later
same-group theme resets tokens omitted by that later theme.

The generated stylesheet metadata carries variable defaults in fallback slots
under `*`, so theme values inherit normally through the DOM:

```css
* {
  --_nanocss_var_hbd9hf--n-default: green;
}
```

Compiled style values reference both the theme slot and the fallback slot:

```ts
{
  color: 'var(--_nanocss_var_hbd9hf, var(--_nanocss_var_hbd9hf--n-default))'
}
```

```ts
// colors.css.ts
export const colors = css.defineVars({
  primary: 'green',
})

const styles = css.create({
  root: {
    color: colors.primary,
  },
})

const theme = css.createTheme(colors, {
  primary: 'red',
})

const resetTheme = css.createTheme(colors, {})

function Component() {
  return <main {...css.props(theme, styles.root)} />
}
```

Variables can use hook values:

```ts
// colors.css.ts
export const colors = css.defineVars({
  primary: {
    default: 'green',
    ':hover': 'lime',
  },
})
```

Variables can derive defaults from other variables in the same file with
zero-argument expression functions. The derived token still gets its own custom
property, so it can be overridden independently by themes.

```ts
// colors.css.ts
export const colors = css.defineVars({
  text: 'black',
  textMuted: () => `color-mix(in srgb, ${colors.text}, transparent 50%)`,
})
```

Derived defaults can reference variables and constants that are defined earlier
in the same file, and can return hook objects. Imported variable or constant
references are rejected for now because NanoCSS Compiler does not do cross-file static
evaluation when emitting stylesheet metadata.

Variables can also be typed with `css.types.*()`. Typed variables emit
`@property` rules so browsers can treat generated custom properties as real CSS
value types for interpolation and validation.

```ts
// tokens.css.ts
export const tokens = css.defineVars({
  accent: css.types.color({
    default: 'blue',
    ':hover': 'red',
  }),
  space: css.types.length(4),
})

const theme = css.createTheme(tokens, {
  accent: css.types.color('purple'),
  space: css.types.length(8),
})
```

The compiler records CSS like:

```css
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
```

Supported type helpers are `angle`, `color`, `url`, `image`, `integer`,
`lengthPercentage`, `length`, `percentage`, `number`, `resolution`, `time`,
`transformFunction`, and `transformList`. Numeric `length` and
`lengthPercentage` values are emitted as pixels; other numeric typed values are
emitted as unitless numbers.

Exported `css.defineVars()` and `css.createTheme()` declarations must live in
`*.css.ts` or `*.css.tsx` files. Import those compiled variable modules through a
`.css`-suffixed specifier so consuming files can treat imported token values as
custom property names:

```ts
import { colors } from './colors.css'
```

Bare numeric variable defaults and theme overrides are rejected because CSS
custom properties do not carry property context. Use strings such as `'4px'`,
`'1'`, or `'0.5'`, or wrap the value in a matching `css.types.*()` helper.

`$$defaults` is reserved by NanoCSS Compiler and cannot be used as a variable token name.

## Constants

`css.defineConsts()` defines exported static constants in `*.css.ts` files. The
compiler replaces local references in supported style positions and leaves a
frozen runtime object for JavaScript imports.

```ts
// tokens.css.ts
export const constants = css.defineConsts({
  compact: '@media (max-width: 700px)',
  sticky: 'sticky',
  swatchSize: 32,
})

const styles = css.create({
  root: {
    width: constants.swatchSize,
    position: css.firstThatWorks(constants.sticky, 'fixed'),
    color: {
      default: 'black',
      [constants.compact]: 'red',
    },
  },
})
```

Only static string and finite number values are supported. Cross-file constant
inlining is intentionally out of scope; JavaScript can still import the frozen
constants object at runtime.

## Compile-Time Env

`css.env` reads JSON values from the transform options at compile time. This is
useful for build-time configuration that must be available while compiling
styles, variables, constants, keyframes, and generated CSS APIs.

```ts
const constants = css.defineConsts({
  compact: css.env.compact,
  swatchSize: css.env['swatch-size'],
})

const styles = css.create({
  root: {
    width: css.env['swatch-size'],
    color: {
      default: css.env.colors.text,
      [css.env.compact]: css.env.colors.compactText,
    },
  },
})
```

With transform options like:

```js
{
  env: {
    compact: '@media (max-width: 700px)',
    'swatch-size': 32,
    colors: {
      text: '#111',
      compactText: '#333',
    },
  },
}
```

`css.env` supports property access and string/number bracket access. Env values
can be strings, finite numbers, booleans, nulls, arrays, or objects. Calls such
as `css.env.colorMix(...)` are rejected; function-valued env is intentionally
not part of the JSON env contract.

For TypeScript, augment `Register` to type your project env:

```ts
declare module 'nanocss-compiler' {
  namespace css {
    interface Register {
      env: {
        compact: string
        'swatch-size': number
        colors: {
          text: string
          compactText: string
        }
      }
    }
  }
}
```

## Keyframes

NanoCSS Compiler includes a StyleX-compatible `keyframes` API.
Static `css.keyframes()` declarations are compiled to animation name strings and
the compiler records generated keyframe CSS in metadata.

```ts
import { css } from 'nanocss-compiler'

const fadeIn = css.keyframes({
  '0%': { opacity: 0 },
  '100%': { opacity: 1 },
})
```

Use the keyframe name in longhand animation properties:

```ts
const styles = css.create({
  root: {
    animationName: fadeIn,
    animationDuration: '1s',
    animationIterationCount: 'infinite',
  },
})
```

For cross-file style use, store generated keyframe names in `css.defineVars()`
and export that variable group from a `*.css.ts` file:

```ts
// animations.css.ts
const fadeIn = css.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
})

export const animations = css.defineVars({
  fadeIn,
})
```

## `css.firstThatWorks()`

`css.firstThatWorks()` expresses CSS fallback values through generated
`@supports` hooks. It accepts static string or number values. The last argument
is the fallback; earlier arguments are tried in order when the browser supports
that declaration.

```ts
const styles = css.create({
  sticky: {
    position: css.firstThatWorks('sticky', '-webkit-sticky', 'fixed'),
  },
})
```

This is equivalent to a hook object shaped like:

```ts
{
  position: {
    default: 'fixed',
    '@supports (position: -webkit-sticky)': '-webkit-sticky',
    '@supports (position: sticky)': 'sticky',
  },
}
```

`css.firstThatWorks()` can also be used in dynamic style functions and
`css.viewTransitionClass()` style sections as long as its arguments are static
strings or numbers.

## Position Try

`css.positionTry()` generates an `@position-try` rule and returns the generated
custom-ident string. Use that string with anchor-positioned styles through
`positionTryFallbacks`.

```ts
const topLeftCorner = css.positionTry({
  positionAnchor: '--anchor',
  top: '0',
  left: '0',
  width: '100px',
  height: '100px',
})

const styles = css.create({
  popover: {
    positionTryFallbacks: topLeftCorner,
  },
})
```

The compiler records CSS like:

```css
@position-try --_nanocss_position_try-b1t7hz {
  position-anchor: --anchor;
  top: 0;
  left: 0;
  width: 100px;
  height: 100px;
}
```

Allowed descriptors are `positionAnchor`, `positionArea`, inset descriptors,
margin descriptors, size descriptors, and `alignSelf`, `justifySelf`, or
`placeSelf`. Descriptors must be static. Variables, constants, and generated
string identifiers such as keyframes or another `positionTry` value are resolved
where static strings are otherwise accepted.

For cross-file use, store the generated fallback string in variables:

```ts
// position-fallbacks.css.ts
const topLeftCorner = css.positionTry({
  top: '0',
  left: '0',
})

export const positionFallbacks = css.defineVars({
  topLeftCorner,
})
```

Directly exported `css.keyframes()` and `css.positionTry()` declarations are
rejected because consuming files cannot safely lower arbitrary imported strings
inside static style objects. Export a `css.defineVars()` group instead.

## View Transitions

`css.viewTransitionClass()` generates a stable view transition class name and
records CSS for view transition pseudo-elements in compiler metadata.

```ts
const fadeIn = css.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
})

export const pageTransition = css.viewTransitionClass({
  group: {
    animationDuration: '250ms',
  },
  old: {
    opacity: {
      default: 1,
      '@media (prefers-reduced-motion: reduce)': 0,
    },
  },
  new: {
    animationName: fadeIn,
    animationDuration: '250ms',
    position: css.firstThatWorks('sticky', 'fixed'),
  },
})
```

Supported sections are `group`, `imagePair`, `old`, and `new`, which map to
`::view-transition-group`, `::view-transition-image-pair`,
`::view-transition-old`, and `::view-transition-new`. Section values must be
static style objects. Hooks, variables, constants, keyframes, and
`css.firstThatWorks()` are supported where they are otherwise supported by the
compiler.

`css.viewTransitionClass()` returns a class-name string, not a composable style
object. Multiple generated view transition classes can be placed in the same
`className`, but class string order does not provide NanoCSS
last-style-wins semantics. If two view transition classes write the same
pseudo-element property, the CSS cascade decides the winner based on generated
rule order and specificity. Prefer a single `css.viewTransitionClass()` value
for each transition shape when properties overlap.

## Hook Stylesheet

Hooks are implemented with CSS custom property fallbacks. The PostCSS extractor
emits the hook stylesheet:

```css
* {
  --_hover-mbscpo-0: initial;
  --_hover-mbscpo-1: ;
}
*:hover {
  --_hover-mbscpo-0: ;
  --_hover-mbscpo-1: initial;
}
```

Compiled inline styles reference those variables:

```ts
{
  color: 'var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)'
}
```

## StyleX Compatibility

NanoCSS Compiler intentionally keeps a StyleX-like API, and can be used behind a local
compatibility module.

```ts
// src/lib/stylex.ts
export { css, css as default } from 'nanocss-compiler'
```

Then configure your bundler alias so `@stylexjs/stylex` resolves to
`src/lib/stylex.ts`, and include that local module in the compiler
`importSources`. The compiler expects namespace-style calls such as
`stylex.create(...)` or `css.create(...)`; destructured calls such as
`create(...)` are not part of the authoring contract.

Implemented StyleX-like APIs include `create`, `props`, `defineVars`,
`createTheme`, `defineConsts`, `keyframes`, `positionTry`, `firstThatWorks`,
`viewTransitionClass`, and `types.*`. Marker-based APIs such as
`stylex.when.*` are not implemented because NanoCSS Compiler currently lowers primarily
to inline styles and would need marker class names plus selector CSS to target
related elements.

## Acknowledgements

- [StyleX](https://github.com/facebook/stylex) for the API shape and styling
  model that NanoCSS Compiler follows.
