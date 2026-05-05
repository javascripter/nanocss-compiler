import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import type * as React from 'react'

type NativeBinding = {
  transformSync: (
    source: string,
    options: NativeBindingTransformOptions,
  ) => NativeBindingTransformResult
}

type NativeBindingTransformOptions = Omit<TransformOptions, 'htmlDefaults' | 'env'> & {
  htmlDefaults?: string
  env?: string
}

export type TransformOptions = {
  filename: string
  code?: boolean
  debug?: boolean
  importSources?: string[]
  inputSourceMap?: string
  htmlDefaults?: Partial<Record<keyof React.JSX.IntrinsicElements, React.CSSProperties>>
  env?: Record<string, unknown>
}

type NativeBindingTransformResult = {
  code?: string
  metadata: {
    nanocss: {
      styleSheet: string
    }
  }
}

type NativeTransformResult = Omit<NativeBindingTransformResult, 'code'> & {
  code: string | null
}

const require = createRequire(import.meta.url)
const currentDir = path.dirname(fileURLToPath(import.meta.url))

function loadNativeBinding(): NativeBinding {
  const candidates = [
    path.resolve(currentDir, 'nanocss_node.node'),
    path.resolve(currentDir, '../dist/nanocss_node.node'),
    path.resolve(currentDir, '../target/debug/nanocss_node.node'),
    path.resolve(currentDir, '../target/release/nanocss_node.node'),
  ]

  const errors: string[] = []
  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) {
      continue
    }

    try {
      return require(candidate) as NativeBinding
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      errors.push(`${candidate}: ${message}`)
    }
  }

  throw new Error(
    [
      '[nanocss] Failed to load native transform.',
      'Run `bun run build:node` to build the NanoCSS Node binding.',
      ...errors,
    ].join('\n'),
  )
}

let binding: NativeBinding | undefined

function transformSync(source: string, options: TransformOptions): NativeTransformResult {
  if (typeof source !== 'string') {
    throw new TypeError('[nanocss] transformSync(...) source must be a string.')
  }

  if (!options || typeof options !== 'object') {
    throw new TypeError('[nanocss] transformSync(...) options are required.')
  }

  if (typeof options.filename !== 'string' || options.filename.length === 0) {
    throw new TypeError('[nanocss] transformSync(...) options.filename is required.')
  }

  binding ??= loadNativeBinding()
  const { htmlDefaults, env, ...nativeOptions } = options
  const result = binding.transformSync(source, {
    ...nativeOptions,
    htmlDefaults:
      htmlDefaults === undefined ? undefined : JSON.stringify(htmlDefaults),
    env: env === undefined ? undefined : JSON.stringify(env),
  })
  return {
    ...result,
    code: result.code ?? null,
  }
}

export { transformSync }
export type { NativeTransformResult as TransformResult }
