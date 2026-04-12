import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TargetCard } from './target-card'

const onToggle = vi.fn()

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => {
      if (key === 'packet_loss') {
        return '丢包率'
      }
      return options?.defaultValue ?? key
    }
  })
}))

describe('TargetCard', () => {
  it('renders packet loss text through i18n', () => {
    render(
      <TargetCard
        color="#0ea5e9"
        onToggle={onToggle}
        target={{
          availability: 0.95,
          avg_latency: 18.2,
          max_latency: 20,
          min_latency: 15,
          packet_loss: 0.05,
          provider: 'ct',
          target_id: 'target-1',
          target_name: '中国电信'
        }}
        visible
      />
    )

    expect(screen.getByText('丢包率 5.0%')).toBeInTheDocument()
  })
})
