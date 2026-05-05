import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import postcss from 'postcss'
import { afterEach, beforeAll, describe, expect, it } from 'vitest'
import createNanoCssPostCssPlugin from '../postcss-plugin'

const tempDirs: string[] = []
const repoRoot = path.resolve(import.meta.dirname, '../..')

function getDebugLibraryName() {
  if (process.platform === 'darwin') {
    return 'libnanocss_node.dylib'
  }
  if (process.platform === 'linux') {
    return 'libnanocss_node.so'
  }
  if (process.platform === 'win32') {
    return 'nanocss_node.dll'
  }
  throw new Error(`Unsupported platform: ${process.platform}`)
}

function createTempProject() {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'nanocss-postcss-'))
  tempDirs.push(cwd)
  fs.mkdirSync(path.join(cwd, 'src'), { recursive: true })
  return cwd
}

function writeFile(cwd: string, file: string, contents: string) {
  const filePath = path.join(cwd, file)
  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, contents)
}

describe('nanocss postcss plugin', () => {
  beforeAll(() => {
    execFileSync('cargo', ['build', '-p', 'nanocss_node'], {
      cwd: repoRoot,
      stdio: 'pipe',
    })
    fs.copyFileSync(
      path.join(repoRoot, 'target/debug', getDebugLibraryName()),
      path.join(repoRoot, 'target/debug/nanocss_node.node'),
    )
  })

  afterEach(() => {
    for (const dir of tempDirs.splice(0)) {
      fs.rmSync(dir, { recursive: true, force: true })
    }
  })

  it('replaces @nanocss with collected compiler css', async () => {
    const cwd = createTempProject()
    writeFile(
      cwd,
      'src/colors.css.ts',
      `
        import { css } from 'nanocss-compiler'

        export const colors = css.defineVars({
          primary: 'green',
        })
      `,
    )
    writeFile(
      cwd,
      'src/component.tsx',
      `
        import { css } from 'nanocss-compiler'

        const fade = css.keyframes({
          from: { opacity: 0 },
          to: { opacity: 1 },
        })

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':hover': 'red',
            },
            animationName: fade,
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
    )

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['src/**/*.{ts,tsx}'],
        debug: true,
      }),
    ]).process('@nanocss;', { from: path.join(cwd, 'src/global.css') })

    expect(result.css).toContain('--_hover-mbscpo-0')
    expect(result.css).toContain('--n-default: green;')
    expect(result.css).toContain('@keyframes __nanocss_keyframes-')
    expect(result.messages).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'dir-dependency',
          glob: '**/*.{ts,tsx}',
        }),
      ]),
    )
  })

  it('collects css from configured custom import sources', async () => {
    const cwd = createTempProject()
    writeFile(
      cwd,
      'src/component.tsx',
      `
        import { css } from '@/lib/css'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':hover': 'red',
            },
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
    )

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['src/**/*.tsx'],
        importSources: ['@/lib/css'],
        debug: true,
      }),
    ]).process('@nanocss;', { from: path.join(cwd, 'src/global.css') })

    expect(result.css).toContain('--_hover-mbscpo-0')
  })

  it('passes env values to the compiler', async () => {
    const cwd = createTempProject()
    writeFile(
      cwd,
      'src/component.tsx',
      `
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: css.env.colors.text,
              [css.env.compact]: css.env.colors.compactText,
            },
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
    )

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['src/**/*.tsx'],
        debug: true,
        env: {
          compact: '@media (max-width: 700px)',
          colors: {
            text: '#111',
            compactText: '#333',
          },
        },
      }),
    ]).process('@nanocss;', { from: path.join(cwd, 'src/global.css') })

    expect(result.css).toContain('@media (max-width: 700px)')
    expect(result.css).toContain('--_media__max-width__700px_-sdukub-0')
  })

  it('uses configured cwd for compiler file identity', async () => {
    const cwd = createTempProject()
    writeFile(
      cwd,
      'apps/web/src/tokens.css.ts',
      `
        import { css } from 'nanocss-compiler'

        const pulse = css.keyframes({
          from: { opacity: 0 },
          to: { opacity: 1 },
        })

        export const animations = css.defineVars({
          pulse,
        })
      `,
    )

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['apps/web/src/**/*.ts'],
        debug: true,
      }),
    ]).process('@nanocss;', {
      from: path.join(cwd, 'apps/web/src/global.css'),
    })

    expect(result.css).toContain('--_nanocss_var_animations_pulse_28n7h5')
  })

  it('supports character classes in include globs', async () => {
    const cwd = createTempProject()
    writeFile(
      cwd,
      'src/component.tsx',
      `
        import { css } from 'nanocss-compiler'

        const styles = css.create({
          root: {
            color: {
              default: 'black',
              ':hover': 'red',
            },
          },
        })

        export function Comp() {
          return <div {...css.props(styles.root)} />
        }
      `,
    )

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['src/**/*.[jt]sx'],
        debug: true,
      }),
    ]).process('@nanocss;', { from: path.join(cwd, 'src/global.css') })

    expect(result.css).toContain('--_hover-mbscpo-0')
    expect(result.messages).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          type: 'dir-dependency',
          glob: '**/*.[jt]sx',
        }),
      ]),
    )
  })

  it('leaves css unchanged when @nanocss is not present', async () => {
    const cwd = createTempProject()

    const result = await postcss([
      createNanoCssPostCssPlugin({
        cwd,
        include: ['src/**/*.{ts,tsx}'],
      }),
    ]).process('body { color: red; }', {
      from: path.join(cwd, 'src/global.css'),
    })

    expect(result.css).toBe('body { color: red; }')
  })
})
