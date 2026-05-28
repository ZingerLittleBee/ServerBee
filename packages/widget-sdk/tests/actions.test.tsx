import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createActionsHelper } from '../src/actions'
import type { ActionDefinition } from '../src/define-widget'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

function installRuntime(overrides: Partial<Parameters<typeof createWidgetRuntime>[0]> = {}) {
  resetRuntime()
  createWidgetRuntime({
    apiBaseUrl: '/api',
    queryClient: {} as any,
    serversStore: () => [],
    themeStore: () => ({ mode: 'light', cssVar: () => '' }),
    onConfigUpdate: () => {},
    ...overrides
  })
}

describe('actions helper', () => {
  beforeEach(() => {
    installRuntime()
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { ok: true } })
    }) as any
  })

  afterEach(() => {
    cleanup()
  })

  it('renders a button that triggers run() and shows loading state', async () => {
    const run = vi.fn().mockResolvedValue(undefined)
    const actions: ActionDefinition[] = [{ id: 'a1', label: 'Do it', run }]
    const helper = createActionsHelper(actions)
    render(helper.render('a1'))
    fireEvent.click(screen.getByRole('button', { name: 'Do it' }))
    await waitFor(() => expect(run).toHaveBeenCalledOnce())
  })

  it('returns null for unknown id', () => {
    const helper = createActionsHelper([])
    const { container } = render(helper.render('missing'))
    expect(container.textContent).toBe('')
  })

  it('calls runtime.requestConfirm when action has confirm and aborts on false', async () => {
    const run = vi.fn().mockResolvedValue(undefined)
    const requestConfirm = vi.fn().mockResolvedValue(false)
    installRuntime({ requestConfirm })
    const actions: ActionDefinition[] = [
      {
        id: 'a1',
        label: 'Restart',
        run,
        confirm: { title: 'Restart server?', body: 'Are you sure?' }
      }
    ]
    const helper = createActionsHelper(actions)
    render(helper.render('a1'))
    fireEvent.click(screen.getByRole('button', { name: 'Restart' }))
    await waitFor(() =>
      expect(requestConfirm).toHaveBeenCalledWith({ title: 'Restart server?', body: 'Are you sure?' })
    )
    expect(run).not.toHaveBeenCalled()
  })

  it('proceeds with action when confirm returns true and emits success notify', async () => {
    const run = vi.fn().mockResolvedValue(undefined)
    const requestConfirm = vi.fn().mockResolvedValue(true)
    const notify = vi.fn()
    installRuntime({ requestConfirm, notify })
    const actions: ActionDefinition[] = [{ id: 'a1', label: 'Restart', run, confirm: { title: 'Restart?' } }]
    const helper = createActionsHelper(actions)
    render(helper.render('a1'))
    fireEvent.click(screen.getByRole('button', { name: 'Restart' }))
    await waitFor(() => expect(run).toHaveBeenCalledOnce())
    await waitFor(() => expect(notify).toHaveBeenCalledWith(expect.objectContaining({ type: 'success' })))
  })

  it('catches errors and emits error notify (does not re-throw)', async () => {
    const run = vi.fn().mockRejectedValue(new Error('boom'))
    const notify = vi.fn()
    installRuntime({ notify })
    const actions: ActionDefinition[] = [{ id: 'a1', label: 'Risky', run }]
    const helper = createActionsHelper(actions)
    render(helper.render('a1'))
    fireEvent.click(screen.getByRole('button', { name: 'Risky' }))
    await waitFor(() =>
      expect(notify).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', message: expect.stringContaining('boom') })
      )
    )
  })
})
