import { cleanup, render, screen } from '@testing-library/react'
import i18next from 'i18next'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { AlertRule, AlertStateResponse } from '@/lib/api-schema'
import { AlertRulesList } from './alert-rules-list'

const SUPERSEDED_LABEL = /Superseded by the next renewal deadline/
const RESOLVED_LABEL = /Resolved/
const TRIGGERED_LABEL = /Triggered \(1x\)/
const CHINESE_SUPERSEDED_LABEL = /已由下一个续费到期日替代/

const rule: AlertRule = {
  id: 'renewal-rule',
  name: 'Renewal reminder',
  enabled: true,
  rules_json: '[]',
  cover_type: 'all',
  trigger_mode: 'once',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z'
}

function renderState(state: AlertStateResponse & { status?: string }) {
  render(
    <AlertRulesList
      deletePending={false}
      deleteRuleId={null}
      expandedRuleId={rule.id}
      isLoading={false}
      onDeleteClose={vi.fn()}
      onDeleteConfirm={vi.fn()}
      onDeleteOpen={vi.fn()}
      onToggleEnabled={vi.fn()}
      onToggleExpanded={vi.fn()}
      rules={[rule]}
      states={[state]}
    />
  )
}

const state: AlertStateResponse = {
  server_id: 'server-1',
  server_name: 'Monthly server',
  count: 1,
  resolved: true,
  resolved_at: null,
  first_triggered_at: '2026-01-31T00:00:00Z',
  last_notified_at: '2026-01-31T00:00:00Z'
}

afterEach(async () => {
  cleanup()
  await i18next.changeLanguage('en')
})

describe('renewal alert state presentation', () => {
  it('shows a superseded occurrence as a neutral deadline transition', () => {
    renderState({ ...state, status: 'superseded' })

    expect(screen.getByText(SUPERSEDED_LABEL)).toBeInTheDocument()
    expect(screen.queryByText(RESOLVED_LABEL)).not.toBeInTheDocument()
  })

  it('preserves firing and resolved labels from older Server responses', () => {
    renderState(state)
    expect(screen.getByText(RESOLVED_LABEL)).toBeInTheDocument()
    cleanup()
    renderState({ ...state, resolved: false })
    expect(screen.getByText(TRIGGERED_LABEL)).toBeInTheDocument()
  })

  it('localizes the superseded occurrence in Chinese', async () => {
    await i18next.changeLanguage('zh')
    renderState({ ...state, status: 'superseded' })
    expect(screen.getByText(CHINESE_SUPERSEDED_LABEL)).toBeInTheDocument()
  })
})
