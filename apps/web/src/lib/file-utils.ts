const EXTENSION_LANGUAGE_MAP: Record<string, string> = {
  yaml: 'yaml',
  yml: 'yaml',
  json: 'json',
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  py: 'python',
  rs: 'rust',
  go: 'go',
  md: 'markdown',
  mdx: 'markdown',
  toml: 'toml',
  ini: 'ini',
  cfg: 'ini',
  conf: 'ini',
  xml: 'xml',
  html: 'html',
  htm: 'html',
  css: 'css',
  scss: 'scss',
  less: 'less',
  sql: 'sql',
  dockerfile: 'dockerfile',
  rb: 'ruby',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  c: 'c',
  cpp: 'cpp',
  h: 'c',
  hpp: 'cpp',
  cs: 'csharp',
  php: 'php',
  lua: 'lua',
  r: 'r',
  pl: 'perl',
  vue: 'html',
  svelte: 'html',
  graphql: 'graphql',
  gql: 'graphql',
  proto: 'protobuf',
  tf: 'hcl',
  hcl: 'hcl'
}

const TEXT_EXTENSIONS = new Set([
  ...Object.keys(EXTENSION_LANGUAGE_MAP),
  'txt',
  'log',
  'csv',
  'tsv',
  'svg',
  'makefile',
  'cmake',
  'gitignore',
  'gitattributes',
  'editorconfig',
  'env'
])

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'ico', 'bmp', 'tiff', 'tif', 'avif'])

const ARCHIVE_EXTENSIONS = new Set(['zip', 'tar', 'gz', 'bz2', 'xz', '7z', 'rar'])
const VIDEO_EXTENSIONS = new Set(['mp4', 'mkv', 'avi', 'mov', 'webm'])
const AUDIO_EXTENSIONS = new Set(['mp3', 'wav', 'ogg', 'flac', 'aac'])

function getExtension(filename: string): string {
  const lower = filename.toLowerCase()
  const lastDot = lower.lastIndexOf('.')
  if (lastDot === -1 || lastDot === lower.length - 1) {
    return ''
  }
  return lower.slice(lastDot + 1)
}

function getBaseName(filename: string): string {
  const parts = filename.split('/')
  const name = parts.at(-1) ?? filename
  return name.toLowerCase()
}

export function extensionToLanguage(filename: string): string {
  const baseName = getBaseName(filename)

  // Handle special filenames without extensions
  if (baseName === 'dockerfile') {
    return 'dockerfile'
  }
  if (baseName === 'makefile' || baseName === 'gnumakefile') {
    return 'makefile'
  }
  if (baseName === 'cmakelists.txt' || baseName.endsWith('.cmake')) {
    return 'cmake'
  }

  const ext = getExtension(filename)
  return EXTENSION_LANGUAGE_MAP[ext] ?? 'plaintext'
}

export function isTextFile(filename: string): boolean {
  const baseName = getBaseName(filename)

  // Special filenames that are text
  if (
    baseName === 'dockerfile' ||
    baseName === 'makefile' ||
    baseName === 'gnumakefile' ||
    baseName === '.gitignore' ||
    baseName === '.gitattributes' ||
    baseName === '.editorconfig' ||
    baseName === '.env' ||
    baseName.startsWith('.env.')
  ) {
    return true
  }

  const ext = getExtension(filename)
  return TEXT_EXTENSIONS.has(ext)
}

export function isImageFile(filename: string): boolean {
  const ext = getExtension(filename)
  return IMAGE_EXTENSIONS.has(ext)
}

export function fileIcon(fileType: string, name: string): string {
  if (fileType === 'Directory') {
    return 'folder'
  }
  if (fileType === 'Symlink') {
    return 'file-symlink'
  }

  const ext = getExtension(name)

  if (IMAGE_EXTENSIONS.has(ext)) {
    return 'file-image'
  }
  if (ext === 'pdf') {
    return 'file-text'
  }
  if (ARCHIVE_EXTENSIONS.has(ext)) {
    return 'file-archive'
  }
  if (VIDEO_EXTENSIONS.has(ext)) {
    return 'file-video'
  }
  if (AUDIO_EXTENSIONS.has(ext)) {
    return 'file-audio'
  }
  if (isTextFile(name)) {
    return 'file-code'
  }

  return 'file'
}

export function joinPath(dir: string, name: string): string {
  return dir.endsWith('/') ? `${dir}${name}` : `${dir}/${name}`
}

export function getFileName(path: string): string {
  return path.split('/').pop() ?? path
}
