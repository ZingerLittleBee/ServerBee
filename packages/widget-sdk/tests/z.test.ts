import { describe, expect, it } from 'vitest'
import { z } from '../src/z'

describe('z primitives', () => {
  it('z.string() parses strings', () => {
    expect(z.string().parse('hi')).toBe('hi')
    expect(() => z.string().parse(1)).toThrow(/string/)
  })

  it('z.number() with min/max', () => {
    const s = z.number().min(0).max(10)
    expect(s.parse(5)).toBe(5)
    expect(() => s.parse(-1)).toThrow(/min/)
    expect(() => s.parse(11)).toThrow(/max/)
  })

  it('z.boolean()', () => {
    expect(z.boolean().parse(true)).toBe(true)
    expect(() => z.boolean().parse('true')).toThrow()
  })

  it('z.enum()', () => {
    const s = z.enum(['a', 'b', 'c'] as const)
    expect(s.parse('a')).toBe('a')
    expect(() => s.parse('d')).toThrow(/enum/)
  })

  it('z.array(inner)', () => {
    const s = z.array(z.number())
    expect(s.parse([1, 2])).toEqual([1, 2])
    expect(() => s.parse([1, 'x'])).toThrow()
  })

  it('z.object({ a, b }) applies defaults and rejects missing', () => {
    const s = z.object({
      a: z.string().default('hello'),
      b: z.number()
    })
    expect(s.parse({ b: 1 })).toEqual({ a: 'hello', b: 1 })
    expect(() => s.parse({ a: 'x' })).toThrow(/b/)
  })

  it('.optional() allows undefined', () => {
    const s = z.string().optional()
    expect(s.parse(undefined)).toBeUndefined()
  })

  it('.describe() attaches label without affecting parse', () => {
    const s = z.string().describe('Server name')
    expect((s as any)._label).toBe('Server name')
    expect(s.parse('x')).toBe('x')
  })
})

describe('z extensions', () => {
  it('z.serverId() validates non-empty string + marks kind', () => {
    const s = z.serverId()
    expect((s as any)._kind).toBe('serverId')
    expect(s.parse('srv-1')).toBe('srv-1')
    expect(() => s.parse('')).toThrow()
  })

  it('z.metricPath() validates dot/bracket path', () => {
    const s = z.metricPath()
    expect(s.parse('cpu.usage')).toBe('cpu.usage')
    expect(s.parse('disks[0].used')).toBe('disks[0].used')
    expect(() => s.parse('--invalid--')).toThrow()
  })

  it('z.color() accepts hex/oklch/rgb strings', () => {
    const s = z.color()
    expect(s.parse('#fff')).toBe('#fff')
    expect(s.parse('oklch(0.5 0 0)')).toBe('oklch(0.5 0 0)')
  })

  it('z.duration() parses 5m / 1h / 30s', () => {
    const s = z.duration()
    expect(s.parse('5m')).toBe('5m')
    expect(s.parse('1h')).toBe('1h')
    expect(() => s.parse('bogus')).toThrow()
  })
})

describe('ZodSchema.introspect', () => {
  it('reports kind/label/default/optional', () => {
    const s = z.string().describe('Name').default('anon').optional()
    const info = s.introspect()
    expect(info.kind).toBe('string')
    expect(info.label).toBe('Name')
    expect(info.default).toBe('anon')
    expect(info.optional).toBe(true)
  })

  it('exposes shape for z.object()', () => {
    const s = z.object({ a: z.string(), b: z.number() })
    const info = s.introspect()
    expect(info.kind).toBe('object')
    expect(info.shape).toBeDefined()
    expect(Object.keys(info.shape ?? {})).toEqual(['a', 'b'])
  })

  it('exposes values for z.enum()', () => {
    const s = z.enum(['x', 'y', 'z'] as const)
    const info = s.introspect()
    expect(info.kind).toBe('enum')
    expect(info.values).toEqual(['x', 'y', 'z'])
  })

  it('reports extension kinds', () => {
    expect(z.metricPath().introspect().kind).toBe('metricPath')
    expect(z.color().introspect().kind).toBe('color')
    expect(z.duration().introspect().kind).toBe('duration')
    expect(z.serverId().introspect().kind).toBe('serverId')
  })
})
