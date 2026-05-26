import fs from 'node:fs'
import path from 'node:path'
import JSZip from 'jszip'

const ROOT = path.resolve('.')
const DIST = path.join(ROOT, 'dist')
const MANIFEST = path.join(ROOT, 'manifest.json')
const PREVIEW = path.join(ROOT, 'public', 'preview.png')

const ALLOWED = new Set([
  'html',
  'htm',
  'js',
  'mjs',
  'css',
  'png',
  'jpg',
  'jpeg',
  'svg',
  'webp',
  'gif',
  'ico',
  'woff',
  'woff2',
  'ttf',
  'otf',
  'json',
  'txt',
  'map'
])
const MAX_FILE = 5 * 1024 * 1024
const MAX_TOTAL = 20 * 1024 * 1024
const MAX_COUNT = 1000

function walk(dir: string, base = ''): { rel: string; abs: string }[] {
  const out: { rel: string; abs: string }[] = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name)
    const rel = path.join(base, entry.name).split(path.sep).join('/')
    if (entry.isDirectory()) {
      out.push(...walk(abs, rel))
    } else {
      out.push({ rel, abs })
    }
  }
  return out
}

const manifest = JSON.parse(fs.readFileSync(MANIFEST, 'utf8'))
const zip = new JSZip()
zip.file('manifest.json', fs.readFileSync(MANIFEST))
if (fs.existsSync(PREVIEW)) {
  zip.file('preview.png', fs.readFileSync(PREVIEW))
}

const distFiles = walk(DIST)
if (distFiles.length + 2 > MAX_COUNT) {
  throw new Error(`too many files (${distFiles.length})`)
}
let total = 0
for (const { rel, abs } of distFiles) {
  const ext = rel.split('.').pop()?.toLowerCase() ?? ''
  if (!ALLOWED.has(ext)) {
    throw new Error(`disallowed extension: ${rel}`)
  }
  const size = fs.statSync(abs).size
  if (size > MAX_FILE) {
    throw new Error(`file too large: ${rel} (${size})`)
  }
  total += size
  if (total > MAX_TOTAL) {
    throw new Error(`total size exceeded: ${total}`)
  }
  zip.file(rel, fs.readFileSync(abs))
}

const out = `${manifest.id}-${manifest.version}.sbtheme`
const blob = await zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' })
fs.writeFileSync(out, blob)
console.log(`wrote ${out} (${blob.byteLength} bytes)`)
