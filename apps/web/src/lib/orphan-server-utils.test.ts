import { describe, expect, it } from 'vitest'
import { countCleanupCandidates, isCleanupCandidate } from './orphan-server-utils'

describe('isCleanupCandidate', () => {
  it('matches unconnected placeholder servers only', () => {
    expect(isCleanupCandidate({ name: 'New Server', online: false, os: null })).toBe(true)
    expect(isCleanupCandidate({ name: 'New Server', online: true, os: null })).toBe(false)
    expect(isCleanupCandidate({ name: 'New Server', online: false, os: 'Linux' })).toBe(false)
    expect(isCleanupCandidate({ name: 'Production', online: false, os: null })).toBe(false)
  })
})

describe('countCleanupCandidates', () => {
  it('counts only offline placeholder rows', () => {
    expect(
      countCleanupCandidates([
        { name: 'New Server', online: false, os: null },
        { name: 'New Server', online: true, os: null },
        { name: 'New Server', online: false, os: 'Linux' }
      ])
    ).toBe(1)
  })
})
