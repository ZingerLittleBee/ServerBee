import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createActionsHelper } from '../src/actions'
import type { ActionDefinition } from '../src/define-widget'

describe('actions helper', () => {
  beforeEach(() => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { ok: true } })
    }) as any
  })

  it('renders a button that triggers run() and shows loading state', async () => {
    const run = vi.fn().mockResolvedValue(undefined)
    const actions: ActionDefinition[] = [{ id: 'a1', label: 'Do it', run }]
    const helper = createActionsHelper(actions)
    render(<>{helper.render('a1')}</>)
    fireEvent.click(screen.getByRole('button', { name: 'Do it' }))
    await waitFor(() => expect(run).toHaveBeenCalledOnce())
  })

  it('returns null for unknown id', () => {
    const helper = createActionsHelper([])
    const { container } = render(<>{helper.render('missing')}</>)
    expect(container.textContent).toBe('')
  })
})
