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
    expect(isImageFile('logo.svg')).toBe(true)
  })
  it('rejects non-images', () => {
    expect(isImageFile('doc.pdf')).toBe(false)
    expect(isImageFile('data.csv')).toBe(false)
  })
})
