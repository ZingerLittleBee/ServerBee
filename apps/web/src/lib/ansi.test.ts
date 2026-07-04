import { describe, expect, it } from 'vitest'
import { stripAnsi } from './ansi'

const ESC = '\u001B'

describe('stripAnsi', () => {
  it('removes SGR color codes', () => {
    expect(stripAnsi(`${ESC}[32mINFO${ESC}[0m ready`)).toBe('INFO ready')
  })

  it('removes dim/bold and nested codes seen in agent logs', () => {
    const raw = `${ESC}[2m2026-07-04T11:00:02Z${ESC}[0m ${ESC}[33m WARN${ESC}[0m ${ESC}[2mserverbee${ESC}[0m: retry`
    expect(stripAnsi(raw)).toBe('2026-07-04T11:00:02Z  WARN serverbee: retry')
  })

  it('removes 256-color and truecolor sequences', () => {
    expect(stripAnsi(`${ESC}[38;5;196mred${ESC}[0m`)).toBe('red')
    expect(stripAnsi(`${ESC}[38;2;255;0;0mred${ESC}[39m`)).toBe('red')
  })

  it('leaves plain text untouched', () => {
    expect(stripAnsi('nginx: worker process started')).toBe('nginx: worker process started')
  })

  it('handles empty input', () => {
    expect(stripAnsi('')).toBe('')
  })
})
