import fs from 'node:fs'
import path from 'node:path'
import postcss from 'postcss'
import { transformSync } from '../native'

type TransformOptions = {
  cwd: string
  debug?: boolean
  importSources?: string[]
  shouldSkipTransformError?: boolean
  env?: Record<string, unknown>
}

const DEFAULT_IMPORT_SOURCES = ['nanocss-compiler']

function createNanoCssBundler() {
  const cssByFile = new Map<string, string>()

  function shouldTransform(sourceCode: string, importSources?: string[]) {
    return (importSources ?? DEFAULT_IMPORT_SOURCES).some((source) =>
      sourceCode.includes(source),
    )
  }

  async function transform(filePath: string, options: TransformOptions) {
    const sourceCode = fs.readFileSync(filePath, 'utf8')
    if (!shouldTransform(sourceCode, options.importSources)) {
      cssByFile.delete(filePath)
      return
    }

    try {
      const result = transformSync(sourceCode, {
        filename: path.relative(options.cwd, filePath),
        code: false,
        debug: options.debug,
        importSources: options.importSources,
        env: options.env,
      })
      const css = result.metadata.nanocss.styleSheet

      if (css) {
        cssByFile.set(filePath, css)
      } else {
        cssByFile.delete(filePath)
      }
    } catch (error) {
      if (options.shouldSkipTransformError) {
        const message = error instanceof Error ? error.message : String(error)
        console.warn(`[postcss-nanocss] Failed to transform "${filePath}": ${message}`)
        return
      }

      throw error
    }
  }

  function remove(filePath: string) {
    cssByFile.delete(filePath)
  }

  function bundle() {
    const css: string[] = []

    css.push(...cssByFile.values())

    if (css.length === 0) {
      return ''
    }

    const seen = new Set<string>()
    const root = postcss.parse(css.join('\n'))
    root.nodes = root.nodes.filter((node) => {
      const value = node.toString()
      if (seen.has(value)) {
        return false
      }
      seen.add(value)
      return true
    })

    return root.toString()
  }

  return { transform, remove, bundle }
}

export { createNanoCssBundler }
