import fs from 'node:fs'
import path from 'node:path'
import type { AtRule, Root } from 'postcss'
import { createNanoCssBundler } from './bundler'
import { getFiles, parseDependency } from './glob'
import type { DependencyMessage, NanoCssPostCssPluginOptions } from './types'

type BuilderOptions = Required<
  Pick<NanoCssPostCssPluginOptions, 'cwd' | 'include' | 'exclude' | 'atRuleName'>
> &
  Pick<NanoCssPostCssPluginOptions, 'debug' | 'importSources' | 'env'>

const DEFAULT_INCLUDE = ['src/**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}']
const DEFAULT_EXCLUDE = [
  '**/*.d.ts',
  '**/*.d.cts',
  '**/*.d.mts',
  '**/node_modules/**',
  '**/dist/**',
]

function normalizeOptions(options: NanoCssPostCssPluginOptions): BuilderOptions {
  return {
    cwd: options.cwd ?? process.cwd(),
    include: options.include ?? DEFAULT_INCLUDE,
    exclude: [...DEFAULT_EXCLUDE, ...(options.exclude ?? [])],
    debug: options.debug,
    importSources: options.importSources,
    env: options.env,
    atRuleName: options.atRuleName ?? 'nanocss',
  }
}

function createNanoCssBuilder() {
  let options = normalizeOptions({})
  const bundler = createNanoCssBundler()
  const fileModifiedMap = new Map<string, number>()

  function configure(nextOptions: NanoCssPostCssPluginOptions) {
    options = normalizeOptions(nextOptions)
  }

  function findAtRule(root: Root): AtRule | null {
    let matchingAtRule: AtRule | null = null
    root.walkAtRules((atRule) => {
      if (atRule.name === options.atRuleName && !atRule.params) {
        matchingAtRule = atRule
      }
    })
    return matchingAtRule
  }

  function getDependencies(): DependencyMessage[] {
    return options.include
      .map((include) => parseDependency(options.cwd, include))
      .filter((dependency): dependency is DependencyMessage => !!dependency)
  }

  async function build({
    shouldSkipTransformError,
  }: {
    shouldSkipTransformError: boolean
  }) {
    const files = getFiles(options.cwd, options.include, options.exclude)
    const fileSet = new Set(files)

    for (const file of fileModifiedMap.keys()) {
      if (!fileSet.has(file)) {
        fileModifiedMap.delete(file)
        bundler.remove(path.resolve(options.cwd, file))
      }
    }

    await Promise.all(
      files.map(async (file) => {
        const filePath = path.resolve(options.cwd, file)
        const modifiedAt = fs.existsSync(filePath)
          ? fs.statSync(filePath).mtimeMs
          : -Infinity
        const previousModifiedAt = fileModifiedMap.get(file)

        if (previousModifiedAt === modifiedAt) {
          return
        }

        fileModifiedMap.set(file, modifiedAt)
        await bundler.transform(filePath, {
          cwd: options.cwd,
          debug: options.debug,
          importSources: options.importSources,
          env: options.env,
          shouldSkipTransformError,
        })
      }),
    )

    return bundler.bundle()
  }

  return { configure, findAtRule, getDependencies, build }
}

export { createNanoCssBuilder }
