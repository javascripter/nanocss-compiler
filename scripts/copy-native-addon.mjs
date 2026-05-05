import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const sourceByPlatform = {
  darwin: 'libnanocss_node.dylib',
  linux: 'libnanocss_node.so',
  win32: 'nanocss_node.dll',
}

const sourceName = sourceByPlatform[process.platform]

if (!sourceName) {
  throw new Error(`Unsupported platform for nanocss native addon: ${process.platform}`)
}

const source = path.join(root, 'target/release', sourceName)
const target = path.join(root, 'dist/nanocss_node.node')

fs.mkdirSync(path.dirname(target), { recursive: true })
fs.copyFileSync(source, target)
