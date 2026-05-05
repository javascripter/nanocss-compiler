type NanoCssPostCssPluginOptions = {
  cwd?: string
  include?: string[]
  exclude?: string[]
  debug?: boolean
  importSources?: string[]
  env?: Record<string, unknown>
  atRuleName?: string
}

type DependencyMessage =
  | {
      type: 'dependency'
      file: string
    }
  | {
      type: 'dir-dependency'
      dir: string
      glob: string
    }

export type {
  NanoCssPostCssPluginOptions,
  DependencyMessage,
}
