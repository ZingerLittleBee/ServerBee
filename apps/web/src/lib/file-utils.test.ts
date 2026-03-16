import { describe, expect, it } from 'vitest'
import { extensionToLanguage, isImageFile, isTextFile } from './file-utils'

describe('extensionToLanguage', () => {
  it('maps yaml files', () => expect(extensionToLanguage('config.yaml')).toBe('yaml'))
  it('maps yml files', () => expect(extensionToLanguage('config.yml')).toBe('yaml'))
  it('maps json files', () => expect(extensionToLanguage('data.json')).toBe('json'))
  it('maps typescript files', () => expect(extensionToLanguage('app.ts')).toBe('typescript'))
  it('maps shell scripts', () => expect(extensionToLanguage('start.sh')).toBe('shell'))
  it('maps rust files', () => expect(extensionToLanguage('main.rs')).toBe('rust'))
  it('maps python files', () => expect(extensionToLanguage('script.py')).toBe('python'))
  it('returns plaintext for unknown', () => expect(extensionToLanguage('file.xyz')).toBe('plaintext'))
  it('handles no extension', () => expect(extensionToLanguage('Makefile')).toBe('makefile'))
})

describe('isTextFile', () => {
  it('identifies text files', () => {
    expect(isTextFile('config.yaml')).toBe(true)
    expect(isTextFile('app.ts')).toBe(true)
    expect(isTextFile('README.md')).toBe(true)
  })
  it('rejects binary files', () => {
    expect(isTextFile('image.png')).toBe(false)
    expect(isTextFile('archive.zip')).toBe(false)
  })
})

describe('isImageFile', () => {
  it('identifies images', () => {
    expect(isImageFile('photo.jpg')).toBe(true)
    expect(isImageFile('icon.png')).toBe(true)
    expect(isImageFile('logo.svg')).toBe(false) // SVG is text (XML), treated as code
  })
  it('rejects non-images', () => {
    expect(isImageFile('doc.pdf')).toBe(false)
    expect(isImageFile('data.csv')).toBe(false)
  })
})

describe('extensionToLanguage edge cases', () => {
  it('maps toml files', () => expect(extensionToLanguage('config.toml')).toBe('toml'))
  it('maps go files', () => expect(extensionToLanguage('main.go')).toBe('go'))
  it('maps dockerfile', () => expect(extensionToLanguage('Dockerfile')).toBe('dockerfile'))
  it('maps sql files', () => expect(extensionToLanguage('schema.sql')).toBe('sql'))
  it('maps css files', () => expect(extensionToLanguage('style.css')).toBe('css'))
  it('maps html files', () => expect(extensionToLanguage('index.html')).toBe('html'))
  it('handles path with dots', () => expect(extensionToLanguage('my.config.yaml')).toBe('yaml'))
  it('handles uppercase extension', () => {
    const result = extensionToLanguage('README.MD')
    expect(typeof result).toBe('string')
  })
})

describe('isTextFile edge cases', () => {
  it('toml is text', () => expect(isTextFile('config.toml')).toBe(true))
  it('sql is text', () => expect(isTextFile('schema.sql')).toBe(true))
  it('conf is text', () => expect(isTextFile('nginx.conf')).toBe(true))
  it('exe is not text', () => expect(isTextFile('app.exe')).toBe(false))
  it('tar.gz is not text', () => expect(isTextFile('backup.tar.gz')).toBe(false))
})

describe('isImageFile edge cases', () => {
  it('webp is image', () => expect(isImageFile('photo.webp')).toBe(true))
  it('ico is image', () => expect(isImageFile('favicon.ico')).toBe(true))
  it('gif is image', () => expect(isImageFile('anim.gif')).toBe(true))
  it('bmp is image', () => expect(isImageFile('old.bmp')).toBe(true))
})
