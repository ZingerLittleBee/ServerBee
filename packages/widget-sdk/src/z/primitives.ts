import { ZError } from './validate'

export type Infer<T> = T extends ZodSchema<infer U> ? U : never
export type ZodTypeAny = ZodSchema<any>

export abstract class ZodSchema<T> {
  abstract _kind: string
  _label?: string
  _default?: T
  _optional = false

  abstract _parse(input: unknown, path: string[]): T

  parse(input: unknown): T {
    if (input === undefined) {
      if (this._default !== undefined) {
        return this._default
      }
      if (this._optional) {
        return undefined as T
      }
    }
    return this._parse(input, [])
  }

  describe(label: string): this {
    this._label = label
    return this
  }

  default(value: T): this {
    this._default = value
    return this
  }

  optional(): this {
    this._optional = true
    return this
  }
}

class ZString extends ZodSchema<string> {
  _kind = 'string'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string') {
      throw new ZError(path, 'expected string')
    }
    return input
  }
}

class ZNumber extends ZodSchema<number> {
  _kind = 'number'
  private _min?: number
  private _max?: number

  min(v: number): this {
    this._min = v
    return this
  }

  max(v: number): this {
    this._max = v
    return this
  }

  _parse(input: unknown, path: string[]): number {
    if (typeof input !== 'number' || Number.isNaN(input)) {
      throw new ZError(path, 'expected number')
    }
    if (this._min !== undefined && input < this._min) {
      throw new ZError(path, `min ${this._min}`)
    }
    if (this._max !== undefined && input > this._max) {
      throw new ZError(path, `max ${this._max}`)
    }
    return input
  }
}

class ZBoolean extends ZodSchema<boolean> {
  _kind = 'boolean'
  _parse(input: unknown, path: string[]): boolean {
    if (typeof input !== 'boolean') {
      throw new ZError(path, 'expected boolean')
    }
    return input
  }
}

class ZEnum<U extends readonly string[]> extends ZodSchema<U[number]> {
  _kind = 'enum'
  values: U

  constructor(values: U) {
    super()
    this.values = values
  }

  _parse(input: unknown, path: string[]): U[number] {
    if (typeof input !== 'string' || !this.values.includes(input as any)) {
      throw new ZError(path, `enum: expected one of ${this.values.join(', ')}`)
    }
    return input as U[number]
  }
}

class ZArray<T> extends ZodSchema<T[]> {
  _kind = 'array'
  inner: ZodSchema<T>

  constructor(inner: ZodSchema<T>) {
    super()
    this.inner = inner
  }

  _parse(input: unknown, path: string[]): T[] {
    if (!Array.isArray(input)) {
      throw new ZError(path, 'expected array')
    }
    return input.map((item, i) => this.inner._parse(item, [...path, String(i)]))
  }
}

class ZObject<Shape extends Record<string, ZodTypeAny>> extends ZodSchema<{
  [K in keyof Shape]: Infer<Shape[K]>
}> {
  _kind = 'object'
  shape: Shape

  constructor(shape: Shape) {
    super()
    this.shape = shape
  }

  _parse(input: unknown, path: string[]) {
    if (!input || typeof input !== 'object' || Array.isArray(input)) {
      throw new ZError(path, 'expected object')
    }
    const obj = input as Record<string, unknown>
    const out: Record<string, unknown> = {}
    for (const key of Object.keys(this.shape)) {
      const schema = this.shape[key]
      const val = obj[key]
      if (val === undefined) {
        if (schema._default !== undefined) {
          out[key] = schema._default
          continue
        }
        if (schema._optional) {
          out[key] = undefined
          continue
        }
        throw new ZError([...path, key], 'required')
      }
      out[key] = schema._parse(val, [...path, key])
    }
    return out as any
  }
}

export const z = {
  string: () => new ZString(),
  number: () => new ZNumber(),
  boolean: () => new ZBoolean(),
  enum: <U extends readonly string[]>(values: U) => new ZEnum(values),
  array: <T>(inner: ZodSchema<T>) => new ZArray(inner),
  object: <S extends Record<string, ZodTypeAny>>(shape: S) => new ZObject(shape)
}
