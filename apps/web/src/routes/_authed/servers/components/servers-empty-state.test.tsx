import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ServersEmptyState, ServersNoResults } from './servers-empty-state'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options?.query === undefined ? key : `${key}:${options.query}`
  })
}))

describe('ServersEmptyState', () => {
  it('explains that no server has ever connected', () => {
    render(<ServersEmptyState />)
    expect(screen.getByText('no_servers_title')).toBeDefined()
    expect(screen.getByText('no_servers_description')).toBeDefined()
  })
})

describe('ServersNoResults', () => {
  it('echoes the query that matched nothing', () => {
    render(<ServersNoResults onClear={() => undefined} query="prod-eu" />)
    expect(screen.getByText('no_results_title:prod-eu')).toBeDefined()
    expect(screen.getByText('no_results_description')).toBeDefined()
  })

  it('offers a way back out of the filtered state', () => {
    const onClear = vi.fn()
    render(<ServersNoResults onClear={onClear} query="prod-eu" />)

    fireEvent.click(screen.getByText('no_results_clear'))
    expect(onClear).toHaveBeenCalledTimes(1)
  })
})
