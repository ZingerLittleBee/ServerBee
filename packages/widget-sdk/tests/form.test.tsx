import { fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'
import { renderConfigForm } from '../src/form'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'
import { z } from '../src/z'

function Wrapper({ schema, initial }: any) {
  const [value, setValue] = useState(initial)
  return <>{renderConfigForm(schema, value, setValue)}</>
}

describe('renderConfigForm', () => {
  beforeEach(() => {
    resetRuntime()
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => [{ id: 's1', name: 'One', online: true, lastSeen: null, capabilities: 0 }],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {}
    })
  })

  it('renders a text input for z.string()', () => {
    const schema = z.object({ name: z.string().describe('Name') })
    render(<Wrapper initial={{ name: 'hi' }} schema={schema} />)
    const input = screen.getByLabelText('Name') as HTMLInputElement
    expect(input.value).toBe('hi')
    fireEvent.change(input, { target: { value: 'world' } })
    expect(input.value).toBe('world')
  })

  it('renders a number input for z.number()', () => {
    const schema = z.object({ count: z.number().describe('Count') })
    render(<Wrapper initial={{ count: 5 }} schema={schema} />)
    expect((screen.getByLabelText('Count') as HTMLInputElement).value).toBe('5')
  })

  it('renders a select for z.enum()', () => {
    const schema = z.object({ mode: z.enum(['a', 'b'] as const).describe('Mode') })
    render(<Wrapper initial={{ mode: 'a' }} schema={schema} />)
    expect(screen.getByLabelText('Mode')).toBeTruthy()
  })

  it('renders a server picker for z.serverId()', () => {
    const schema = z.object({ srv: z.serverId().describe('Server') })
    render(<Wrapper initial={{ srv: 's1' }} schema={schema} />)
    expect(screen.getByLabelText('Server')).toBeTruthy()
  })
})
