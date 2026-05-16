import { describe, expect, it } from 'vitest'
import { resolveInitialServersView } from './mobile-view'

describe('resolveInitialServersView', () => {
  it('uses grid as the mobile default when there is no saved preference', () => {
    expect(resolveInitialServersView({ isMobile: true, searchView: undefined, storedView: null })).toBe('grid')
  })

  it('uses table as the desktop default when there is no saved preference', () => {
    expect(resolveInitialServersView({ isMobile: false, searchView: undefined, storedView: null })).toBe('table')
  })

  it('keeps explicit route search above viewport defaults', () => {
    expect(resolveInitialServersView({ isMobile: true, searchView: 'table', storedView: 'grid' })).toBe('table')
  })

  it('keeps the saved preference above viewport defaults', () => {
    expect(resolveInitialServersView({ isMobile: true, searchView: undefined, storedView: 'table' })).toBe('table')
  })
})
