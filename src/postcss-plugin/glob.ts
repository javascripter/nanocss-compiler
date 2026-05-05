import fs from 'node:fs'
import path from 'node:path'
import type { DependencyMessage } from './types'

function normalizePath(filePath: string) {
  return filePath.split(path.sep).join('/')
}

function hasGlob(pattern: string) {
  return /[*?{}[\]]/.test(pattern)
}

function escapeRegExp(value: string) {
  return value.replace(/[|\\{}()[\]^$+?.]/g, '\\$&')
}

function escapeCharacterClass(value: string) {
  return value.replace(/[\\\]]/g, '\\$&')
}

function globToRegExp(pattern: string) {
  let source = ''

  for (let index = 0; index < pattern.length; index++) {
    const char = pattern[index]
    const next = pattern[index + 1]

    if (char === '*') {
      if (next === '*') {
        const after = pattern[index + 2]
        if (after === '/') {
          source += '(?:.*/)?'
          index += 2
        } else {
          source += '.*'
          index += 1
        }
      } else {
        source += '[^/]*'
      }
      continue
    }

    if (char === '?') {
      source += '[^/]'
      continue
    }

    if (char === '[') {
      const end = pattern.indexOf(']', index + 1)
      if (end !== -1) {
        let classValue = pattern.slice(index + 1, end)
        let negated = false
        if (classValue.startsWith('!') || classValue.startsWith('^')) {
          negated = true
          classValue = classValue.slice(1)
        }
        if (classValue.length > 0) {
          source += `[${negated ? '^' : ''}${escapeCharacterClass(classValue)}]`
          index = end
          continue
        }
      }
    }

    if (char === '{') {
      const end = pattern.indexOf('}', index + 1)
      if (end !== -1) {
        const options = pattern
          .slice(index + 1, end)
          .split(',')
          .map(escapeRegExp)
          .join('|')
        source += `(?:${options})`
        index = end
        continue
      }
    }

    source += escapeRegExp(char)
  }

  return new RegExp(`^${source}$`)
}

function getGlobBase(pattern: string) {
  const normalized = normalizePath(pattern)
  const firstGlobIndex = normalized.search(/[*?{}[\]]/)

  if (firstGlobIndex === -1) {
    return path.dirname(pattern)
  }

  const slashIndex = normalized.lastIndexOf('/', firstGlobIndex)
  if (slashIndex === -1) {
    return '.'
  }

  return normalized.slice(0, slashIndex) || '.'
}

function parseDependency(cwd: string, pattern: string): DependencyMessage | null {
  if (pattern.startsWith('!')) {
    return null
  }

  if (!hasGlob(pattern)) {
    return {
      type: 'dependency',
      file: path.normalize(path.resolve(cwd, pattern)),
    }
  }

  const base = getGlobBase(pattern)
  let glob = normalizePath(pattern).slice(normalizePath(base).length)
  if (glob.startsWith('/')) {
    glob = glob.slice(1)
  }

  return {
    type: 'dir-dependency',
    dir: path.normalize(path.resolve(cwd, base)),
    glob,
  }
}

function walkFiles(dir: string, files: string[] = []) {
  if (!fs.existsSync(dir)) {
    return files
  }

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const filePath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      walkFiles(filePath, files)
    } else if (entry.isFile()) {
      files.push(filePath)
    }
  }

  return files
}

function getFiles(cwd: string, include: string[], exclude: string[]) {
  const excludePatterns = exclude.map((pattern) =>
    globToRegExp(normalizePath(pattern)),
  )
  const files = new Set<string>()

  for (const pattern of include) {
    if (pattern.startsWith('!')) {
      continue
    }

    if (!hasGlob(pattern)) {
      const absolutePath = path.resolve(cwd, pattern)
      if (fs.existsSync(absolutePath) && fs.statSync(absolutePath).isFile()) {
        files.add(path.relative(cwd, absolutePath))
      }
      continue
    }

    const base = path.resolve(cwd, getGlobBase(pattern))
    const matcher = globToRegExp(normalizePath(pattern))

    for (const filePath of walkFiles(base)) {
      const relativePath = normalizePath(path.relative(cwd, filePath))
      if (matcher.test(relativePath)) {
        files.add(relativePath)
      }
    }
  }

  return Array.from(files)
    .filter((file) => {
      const normalized = normalizePath(file)
      return !excludePatterns.some((pattern) => pattern.test(normalized))
    })
    .sort()
}

export { getFiles, parseDependency }
