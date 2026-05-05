import type * as React from 'react'

export type Falsy = undefined | null | false

type HookNames = `:${string}` | `@${string}` | `[${string}`

type PropertyValue<T> =
  | T
  | null
  | ({
      default: T | null
    } & { [K in HookNames]?: PropertyValue<T> })

declare const typedVariableValueBrand: unique symbol

export type TypedVariableValue = {
  readonly [typedVariableValueBrand]: never
}

type TypedVariableInput =
  | string
  | number
  | ({
      default: string | number
    } & { [K in HookNames]?: TypedVariableInput })

type StylePropertyValue<T> =
  | T
  | null
  | FirstThatWorksValue<T>
  | ({
      default: T | null
    } & { [K in HookNames]?: StylePropertyValue<T> })

export type StyleObject = {
  [K in keyof NanoCSSProperties]?:
    | StylePropertyValue<NanoCSSProperties[K]>
    | undefined
}

type StyleFunction = (...args: any[]) => StyleObject

type StyleDefinitions = Record<string, StyleObject | StyleFunction>

type ViewTransitionClassOptions = {
  // Keep sections aligned with view-transition-class pseudo-elements:
  // https://developer.mozilla.org/docs/Web/CSS/view-transition-class
  group?: StyleObject
  imagePair?: StyleObject
  old?: StyleObject
  new?: StyleObject
}

// Keep descriptor names aligned with CSSPositionTryDescriptors:
// https://drafts.csswg.org/css-anchor-position-1/#the-csspositiontryrule-interface
type PositionTryProperty =
  | 'positionAnchor'
  | 'positionArea'
  | 'top'
  | 'right'
  | 'bottom'
  | 'left'
  | 'inset'
  | 'insetBlock'
  | 'insetBlockStart'
  | 'insetBlockEnd'
  | 'insetInline'
  | 'insetInlineStart'
  | 'insetInlineEnd'
  | 'margin'
  | 'marginTop'
  | 'marginRight'
  | 'marginBottom'
  | 'marginLeft'
  | 'marginBlock'
  | 'marginBlockStart'
  | 'marginBlockEnd'
  | 'marginInline'
  | 'marginInlineStart'
  | 'marginInlineEnd'
  | 'width'
  | 'minWidth'
  | 'maxWidth'
  | 'height'
  | 'minHeight'
  | 'maxHeight'
  | 'blockSize'
  | 'minBlockSize'
  | 'maxBlockSize'
  | 'inlineSize'
  | 'minInlineSize'
  | 'maxInlineSize'
  | 'alignSelf'
  | 'justifySelf'
  | 'placeSelf'

type PositionTryOptions = Partial<
  Record<
    PositionTryProperty,
    StylePropertyValue<NanoCSSProperties[keyof NanoCSSProperties]>
  >
>

type TokenValue =
  | PropertyValue<NanoCSSProperties[keyof NanoCSSProperties]>
  | TypedVariableValue

type Tokens = {
  [key: string]: TokenValue | (() => TokenValue)
}

type Consts = Record<string, string | number>

export interface Register {}

type Env = Register extends { env: infer TEnv }
  ? TEnv
  : Readonly<{ [key: string]: unknown }>

type VarGroup<DefaultTokens extends Tokens> = {
  [Key in keyof DefaultTokens]: string
}

export type NanoCSSProperties = React.CSSProperties & {
  positionAnchor?: string
  positionArea?: string
  positionTryFallbacks?: string
  positionTryOrder?: string
}

export type CompiledStyle = NanoCSSProperties

export type StyleProp = CompiledStyle | ReadonlyArray<StyleProp> | Falsy

declare const firstThatWorksBrand: unique symbol

type FirstThatWorksValue<T> = T & {
  readonly [firstThatWorksBrand]: never
}

type ValidateStyleKeys<S extends StyleDefinitions> = {
  [K in keyof S]: S[K] extends (...args: any[]) => infer R
    ? Exclude<keyof R, keyof React.CSSProperties> extends never
      ? S[K]
      : never
    : Exclude<keyof S[K], keyof React.CSSProperties> extends never
      ? S[K]
      : never
}

const compilerSetupMessage =
  'Configure nanocss-compiler/swc so NanoCSS compile-time APIs are compiled away.'

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function props(..._styles: StyleProp[]): {
  className?: string
  style?: NanoCSSProperties
} {
  throw new Error(
    `[nanocss] css.props(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function create<const S extends StyleDefinitions>(
  _styles: S & ValidateStyleKeys<S>,
): {
  [K in keyof S]: S[K] extends (...args: infer Args) => StyleObject
    ? (...args: Args) => CompiledStyle
    : CompiledStyle
} {
  throw new Error(
    `[nanocss] css.create(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function defineVars<DefaultTokens extends Tokens>(
  _tokens: DefaultTokens,
): VarGroup<DefaultTokens> {
  throw new Error(
    `[nanocss] css.defineVars(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function defineConsts<const C extends Consts>(_consts: C): Readonly<C> {
  throw new Error(
    `[nanocss] css.defineConsts(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function createTheme<Vars extends Record<string, string>>(
  _baseTokens: Vars,
  _overrides: Partial<{
    [Key in keyof Vars]:
      | PropertyValue<NanoCSSProperties[keyof NanoCSSProperties]>
      | TypedVariableValue
  }>,
): CompiledStyle {
  throw new Error(
    `[nanocss] css.createTheme(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function keyframes(_frames: Record<string, React.CSSProperties>): string {
  throw new Error(
    `[nanocss] css.keyframes(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function positionTry(_options: PositionTryOptions): string {
  throw new Error(
    `[nanocss] css.positionTry(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function viewTransitionClass(_options: ViewTransitionClassOptions): string {
  throw new Error(
    `[nanocss] css.viewTransitionClass(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

/**
 * Compile-time API. Runtime calls indicate the NanoCSS compiler did not run.
 */
function firstThatWorks<const Values extends Array<string | number>>(
  ..._values: Values
): FirstThatWorksValue<Values[number]> {
  throw new Error(
    `[nanocss] css.firstThatWorks(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

function cssType(_value: TypedVariableInput): TypedVariableValue {
  throw new Error(
    `[nanocss] css.types.*(...) is a compile-time API. ${compilerSetupMessage}`,
  )
}

const types = {
  // Keep syntax names aligned with CSS Properties and Values API supported names:
  // https://drafts.css-houdini.org/css-properties-values-api-1/#supported-names
  angle: cssType,
  color: cssType,
  url: cssType,
  image: cssType,
  integer: cssType,
  lengthPercentage: cssType,
  length: cssType,
  percentage: cssType,
  number: cssType,
  resolution: cssType,
  time: cssType,
  transformFunction: cssType,
  transformList: cssType,
}

const env = Object.freeze({}) as Env

export {
  create,
  createTheme,
  defineConsts,
  defineVars,
  env,
  firstThatWorks,
  keyframes,
  positionTry,
  props,
  types,
  viewTransitionClass,
}
