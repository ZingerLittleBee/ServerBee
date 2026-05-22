import { render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { UnlockService } from '@/lib/ip-quality-types'

// Mock the API hooks so the dialog can render without a QueryClient and so we
// can assert on the mutation payloads. Mirrors how use-ip-quality-api.test.ts
// mocks the fetch layer — here we mock one level higher, at the hook boundary.
const createMutate = vi.fn()
const updateMutate = vi.fn()

vi.mock('@/hooks/use-ip-quality-api', () => ({
  useCreateService: () => ({ mutate: createMutate, isPending: false }),
  useUpdateService: () => ({ mutate: updateMutate, isPending: false })
}))

import { CustomServiceDialog, parseExistingRules, parseRequest, toNumber } from './custom-service-dialog'

beforeEach(() => {
  createMutate.mockClear()
  updateMutate.mockClear()
})

afterEach(() => {
  vi.clearAllMocks()
})

function makeService(overrides: Partial<UnlockService> = {}): UnlockService {
  return {
    id: 'svc-1',
    key: 'custom_abc12345',
    name: 'My Probe',
    category: 'streaming',
    popularity: 42,
    is_builtin: false,
    enabled: true,
    detector: null,
    request: JSON.stringify({
      url: 'https://example.com/probe',
      method: 'HEAD',
      headers: [['X-Test', 'abc']],
      timeout_ms: 8000
    }),
    rules: JSON.stringify([{ match: { kind: 'status_equals', code: 200 }, result: 'unlocked' }]),
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides
  }
}

describe('toNumber', () => {
  it('parses a normal integer string', () => {
    expect(toNumber('200')).toBe(200)
  })

  it('falls back to 0 for a partially-typed value', () => {
    expect(toNumber('-')).toBe(0)
    expect(toNumber('e')).toBe(0)
    expect(toNumber('')).toBe(0)
  })
})

describe('parseRequest', () => {
  it('parses a well-formed request JSON', () => {
    const parsed = parseRequest(makeService())
    expect(parsed.url).toBe('https://example.com/probe')
    expect(parsed.method).toBe('HEAD')
    expect(parsed.timeout_ms).toBe(8000)
    expect(parsed.headers).toEqual([['X-Test', 'abc']])
  })

  it('returns defaults when the service is null', () => {
    const parsed = parseRequest(null)
    expect(parsed).toEqual({ url: '', method: 'GET', timeout_ms: 5000, headers: [] })
  })

  it('returns defaults when the service has no request', () => {
    const parsed = parseRequest(makeService({ request: null }))
    expect(parsed).toEqual({ url: '', method: 'GET', timeout_ms: 5000, headers: [] })
  })

  it('falls back to defaults on malformed JSON', () => {
    const parsed = parseRequest(makeService({ request: '{not json' }))
    expect(parsed).toEqual({ url: '', method: 'GET', timeout_ms: 5000, headers: [] })
  })

  it('defaults headers to an empty array when the field is missing', () => {
    const parsed = parseRequest(makeService({ request: JSON.stringify({ url: 'https://x.test' }) }))
    expect(parsed.headers).toEqual([])
    expect(parsed.method).toBe('GET')
    expect(parsed.timeout_ms).toBe(5000)
  })
})

describe('parseExistingRules', () => {
  it('parses a well-formed rules array', () => {
    const rules = parseExistingRules(makeService())
    expect(rules).toHaveLength(1)
    expect(rules[0].match).toEqual({ kind: 'status_equals', code: 200 })
    expect(rules[0].result).toBe('unlocked')
  })

  it('falls back to one default rule when rules is null', () => {
    const rules = parseExistingRules(makeService({ rules: null }))
    expect(rules).toHaveLength(1)
    expect(rules[0].result).toBe('unlocked')
  })

  it('falls back to one default rule on an empty array', () => {
    const rules = parseExistingRules(makeService({ rules: '[]' }))
    expect(rules).toHaveLength(1)
  })

  it('falls back to one default rule on malformed JSON', () => {
    const rules = parseExistingRules(makeService({ rules: '[bad' }))
    expect(rules).toHaveLength(1)
  })
})

describe('CustomServiceDialog rendering', () => {
  it('renders create mode with an empty form and at least one default rule row', () => {
    render(<CustomServiceDialog onOpenChange={() => undefined} open={true} />)
    expect(screen.getByText('New custom service')).toBeInTheDocument()
    expect(screen.getByLabelText('Name')).toHaveValue('')
    expect(screen.getByLabelText('URL')).toHaveValue('')
    expect(screen.getAllByTestId('rule-row').length).toBeGreaterThanOrEqual(1)
  })

  it('renders edit mode seeded from an existing service', () => {
    render(<CustomServiceDialog onOpenChange={() => undefined} open={true} service={makeService()} />)
    expect(screen.getByText('Edit custom service')).toBeInTheDocument()
    expect(screen.getByLabelText('Name')).toHaveValue('My Probe')
    expect(screen.getByLabelText('URL')).toHaveValue('https://example.com/probe')
    expect(screen.getByLabelText('Timeout (ms)')).toHaveValue(8000)
    // Existing header row is seeded.
    expect(screen.getByLabelText('Header name')).toHaveValue('X-Test')
    expect(screen.getByLabelText('Header value')).toHaveValue('abc')
    // The single existing rule is seeded.
    expect(screen.getAllByTestId('rule-row')).toHaveLength(1)
  })
})

describe('CustomServiceDialog submission', () => {
  it('calls the create mutation with the expected payload in create mode', () => {
    render(<CustomServiceDialog onOpenChange={() => undefined} open={true} />)

    const name = screen.getByLabelText('Name')
    name.focus()
    fireInput(name, 'My New Service')
    const url = screen.getByLabelText('URL')
    fireInput(url, 'https://new.example.com')

    screen.getByRole('button', { name: 'Create' }).click()

    expect(createMutate).toHaveBeenCalledTimes(1)
    expect(updateMutate).not.toHaveBeenCalled()
    const [payload] = createMutate.mock.calls[0]
    expect(payload).toMatchObject({
      name: 'My New Service',
      url: 'https://new.example.com',
      category: 'streaming',
      method: 'GET',
      timeout_ms: 5000
    })
    expect(Array.isArray(payload.rules)).toBe(true)
    expect(payload.rules.length).toBeGreaterThanOrEqual(1)
    expect(payload).not.toHaveProperty('id')
  })

  it('calls the update mutation with the service id in edit mode', () => {
    render(<CustomServiceDialog onOpenChange={() => undefined} open={true} service={makeService()} />)

    screen.getByRole('button', { name: 'Save' }).click()

    expect(updateMutate).toHaveBeenCalledTimes(1)
    expect(createMutate).not.toHaveBeenCalled()
    const [payload] = updateMutate.mock.calls[0]
    expect(payload).toMatchObject({
      id: 'svc-1',
      name: 'My Probe',
      url: 'https://example.com/probe',
      method: 'HEAD',
      timeout_ms: 8000
    })
    expect(payload.headers).toEqual([['X-Test', 'abc']])
  })
})

// React's controlled inputs need the native value setter to register changes.
function fireInput(element: HTMLElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')?.set
  setter?.call(element, value)
  element.dispatchEvent(new Event('input', { bubbles: true }))
}
