import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import i18next from 'i18next'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerResponse } from '@/lib/api-schema'
import { ServerEditDialog } from './server-edit-dialog'

const apiBoundary = vi.hoisted(() => ({ get: vi.fn(), put: vi.fn() }))
const CALENDAR_DATES = [{ date: '2026-01-31' }, { date: '2024-02-29' }]
const MISSING_PREREQUISITES = [
  { label: 'date', billingCycle: 'monthly', date: null, timezone: 'America/New_York' },
  { label: 'interval', billingCycle: null, date: '2026-03-08', timezone: 'America/New_York' },
  { label: 'supported interval', billingCycle: 'weekly', date: '2026-03-08', timezone: 'America/New_York' },
  { label: 'valid timezone', billingCycle: 'monthly', date: '2026-03-08', timezone: 'Mars/Olympus' },
  { label: 'IANA timezone', billingCycle: 'monthly', date: '2026-03-08', timezone: '+01:00' }
]

// jsdom does not implement the browser animation boundary used by ScrollArea.
Element.prototype.getAnimations = () => []

vi.mock('@/lib/api-client', () => ({ api: apiBoundary }))
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }))

const server = {
  agent_authority: { outstanding_offer: null, status: 'claimed' },
  billing_cycle: 'monthly',
  billing_start_day: null,
  capabilities: 56,
  created_at: '2026-04-18T00:00:00Z',
  currency: 'USD',
  expired_at: '2026-03-09T03:59:59Z',
  features: [],
  geo_manual: false,
  group_id: null,
  has_token: true,
  hidden: false,
  id: 'server-1',
  name: 'New York Edge',
  price: null,
  protocol_version: 2,
  public_remark: null,
  remark: null,
  renewal: {
    billing_timezone: 'America/New_York',
    confirmed_expired_at: '2026-02-09T04:59:59Z',
    deadline_origin: 'projected',
    enabled: false,
    expiry_date: '2026-03-08',
    occurrence_id: 'occurrence-1'
  },
  traffic_limit: null,
  traffic_limit_type: 'sum',
  updated_at: '2026-04-18T00:00:00Z',
  weight: 100
} satisfies ServerResponse

function renderEditor(value: ServerResponse = server) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const view = render(
    <QueryClientProvider client={client}>
      <ServerEditDialog onClose={vi.fn()} open server={value} />
    </QueryClientProvider>
  )
  return {
    ...view,
    updateServer(updated: ServerResponse) {
      view.rerender(
        <QueryClientProvider client={client}>
          <ServerEditDialog onClose={vi.fn()} open server={updated} />
        </QueryClientProvider>
      )
    }
  }
}

beforeEach(() => {
  apiBoundary.get.mockResolvedValue([])
  apiBoundary.put.mockResolvedValue(server)
})

afterEach(async () => {
  cleanup()
  await i18next.changeLanguage('en')
  vi.clearAllMocks()
})

describe('server renewal date editing', () => {
  it('does not confirm the previous deadline when the catalog advances while the editor stays open', async () => {
    const original = { ...server, renewal: { ...server.renewal, enabled: true } }
    const editor = renderEditor(original)
    editor.updateServer({
      ...original,
      expired_at: '2026-04-09T03:59:59Z',
      renewal: { ...original.renewal, expiry_date: '2026-04-08', occurrence_id: 'occurrence-2' }
    })
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), { target: { value: 'Renamed after advancement' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ name: 'Renamed after advancement' })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it('preserves explicit date, timezone and switch edits across a catalog refresh', async () => {
    const original = { ...server, renewal: { ...server.renewal, enabled: true } }
    const editor = renderEditor(original)
    fireEvent.change(screen.getByLabelText('Expiration date'), { target: { value: '2026-03-10' } })
    fireEvent.change(screen.getByLabelText('Billing timezone'), { target: { value: 'Asia/Tokyo' } })
    fireEvent.click(screen.getByRole('switch', { name: 'Automatic renewal tracking' }))
    editor.updateServer({
      ...original,
      expired_at: '2026-04-09T03:59:59Z',
      renewal: { ...original.renewal, expiry_date: '2026-04-08', occurrence_id: 'occurrence-2' }
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({
      enabled: false,
      expiry_date: '2026-03-10',
      billing_timezone: 'Asia/Tokyo'
    })
  })

  it('omits restored renewal fields even when live props now have a different date, timezone and switch', async () => {
    const original = { ...server, renewal: { ...server.renewal, enabled: true } }
    const editor = renderEditor(original)
    const date = screen.getByLabelText('Expiration date')
    const timezone = screen.getByLabelText('Billing timezone')
    const automatic = screen.getByRole('switch', { name: 'Automatic renewal tracking' })
    fireEvent.change(date, { target: { value: '2026-03-10' } })
    fireEvent.change(date, { target: { value: '2026-03-08' } })
    fireEvent.change(timezone, { target: { value: 'Asia/Tokyo' } })
    fireEvent.change(timezone, { target: { value: 'America/New_York' } })
    fireEvent.click(automatic)
    fireEvent.click(automatic)
    editor.updateServer({
      ...original,
      expired_at: '2026-04-08T14:59:59Z',
      renewal: {
        ...original.renewal,
        enabled: false,
        billing_timezone: 'Asia/Tokyo',
        expiry_date: '2026-04-08',
        occurrence_id: 'occurrence-2'
      }
    })
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Price' }), { target: { value: '15' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ price: 15 })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it('does not confirm a clamped projection after restoring its selected date', async () => {
    renderEditor({
      ...server,
      expired_at: '2026-03-01T04:59:59.999999999Z',
      renewal: { ...server.renewal, enabled: true, expiry_date: '2026-02-28' }
    })
    const selectedDate = screen.getByLabelText('Expiration date')
    fireEvent.change(selectedDate, { target: { value: '2026-03-01' } })
    fireEvent.change(selectedDate, { target: { value: '2026-02-28' } })
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Price' }), { target: { value: '15' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ price: 15, billing_cycle: 'monthly' })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it('changes the interval of an active clamped projection without sending a date', async () => {
    renderEditor({ ...server, renewal: { ...server.renewal, enabled: true, expiry_date: '2026-02-28' } })
    fireEvent.click(screen.getByText('Monthly'))
    const interval = await screen.findByRole('option', { name: 'Quarterly' })
    fireEvent.mouseMove(interval)
    fireEvent.click(interval)
    expect(screen.getByLabelText('Expiration date')).toHaveValue('2026-02-28')
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ billing_cycle: 'quarterly' })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it('corrects a frozen deadline and timezone using the existing billing form', async () => {
    renderEditor({ ...server, renewal: { ...server.renewal, deadline_origin: 'frozen' } })
    expect(screen.getByText('Frozen projected deadline')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Billing timezone'), { target: { value: 'Asia/Tokyo' } })
    fireEvent.change(screen.getByLabelText('Expiration date'), { target: { value: '2026-03-15' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({
      billing_timezone: 'Asia/Tokyo',
      expiry_date: '2026-03-15'
    })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it('preserves legacy-server saves without exposing an unsupported automatic switch', async () => {
    renderEditor({ ...server, renewal: undefined })
    expect(screen.queryByRole('switch', { name: 'Automatic renewal tracking' })).not.toBeInTheDocument()
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), { target: { value: 'Legacy rename' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it.each(MISSING_PREREQUISITES)('cannot opt in without a $label', (input) => {
    renderEditor({
      ...server,
      billing_cycle: input.billingCycle,
      renewal: { ...server.renewal, expiry_date: input.date, billing_timezone: input.timezone }
    })
    const automaticRenewal = screen.getByRole('switch', { name: 'Automatic renewal tracking' })
    expect(automaticRenewal).toHaveAttribute('aria-disabled', 'true')
    fireEvent.click(automaticRenewal)
    expect(automaticRenewal).not.toBeChecked()
    expect(automaticRenewal).toHaveAccessibleDescription(
      'Choose an expiry date, a monthly, quarterly or yearly interval, and a valid IANA billing timezone.'
    )
  })

  it('omits renewal intent after restoring the original automatic setting', async () => {
    renderEditor({ ...server, renewal: { ...server.renewal, enabled: true } })
    const automaticRenewal = screen.getByRole('switch', { name: 'Automatic renewal tracking' })
    fireEvent.click(automaticRenewal)
    fireEvent.click(automaticRenewal)
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Price' }), { target: { value: '12.50' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it('distinguishes the projected deadline from confirmed history and explains the forecast', () => {
    renderEditor({ ...server, renewal: { ...server.renewal, enabled: true } })

    expect(screen.getByText('Projected deadline')).toBeInTheDocument()
    expect(screen.getByText('Last operator-confirmed expiry: 2/8/2026 (America/New_York)')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Tracking advances the forecast after each expiry date ends. Disabling freezes the current deadline. This estimate does not confirm provider renewal or payment.'
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Renewal deadline is separate from the cost estimation period.')).toBeInTheDocument()
  })

  it('blocks an enabled configuration with a cleared date until tracking is disabled', async () => {
    renderEditor({ ...server, renewal: { ...server.renewal, enabled: true } })
    fireEvent.click(screen.getByRole('button', { name: 'Clear expiration date' }))
    fireEvent.change(screen.getByLabelText('Billing timezone'), { target: { value: '' } })
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Choose an expiry date, a monthly, quarterly or yearly interval, and a valid IANA billing timezone.'
    )
    fireEvent.click(screen.getByRole('switch', { name: 'Automatic renewal tracking' }))
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({
      enabled: false,
      expiry_date: null,
      billing_timezone: null
    })
  })

  it('opts in to automatic renewal without resubmitting the selected date or timezone', async () => {
    renderEditor()
    const automaticRenewal = screen.getByRole('switch', { name: 'Automatic renewal tracking' })
    expect(automaticRenewal).not.toBeChecked()
    fireEvent.click(automaticRenewal)
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ enabled: true })
  })

  it('selects a billing date even when that date is skipped in the browser timezone', async () => {
    renderEditor({
      ...server,
      expired_at: '2011-12-30T00:00:00Z',
      renewal: { ...server.renewal, billing_timezone: 'UTC', expiry_date: '2011-12-29' }
    })
    const selectedDate = screen.getByLabelText('Expiration date')
    expect(selectedDate).toHaveValue('2011-12-29')
    fireEvent.change(selectedDate, { target: { value: '2011-12-30' } })
    expect(selectedDate).toHaveValue('2011-12-30')
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ expiry_date: '2011-12-30' })
  })

  it('displays the server-selected local expiry date instead of its UTC day', () => {
    renderEditor()

    expect(screen.getByLabelText('Expiration date')).toHaveValue('2026-03-08')
  })

  it('preserves the existing deadline when saving an unrelated name change', async () => {
    renderEditor()
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), { target: { value: 'Renamed Edge' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    const [path, payload] = apiBoundary.put.mock.calls[0]
    expect(path).toBe('/api/servers/server-1')
    expect(payload).toMatchObject({ name: 'Renamed Edge' })
    expect(payload).not.toHaveProperty('expired_at')
    expect(payload).not.toHaveProperty('renewal')
  })

  it('submits an explicitly selected expiry as a local date without a UTC timestamp', async () => {
    renderEditor()
    fireEvent.change(screen.getByLabelText('Expiration date'), { target: { value: '2026-03-10' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    const payload = apiBoundary.put.mock.calls[0][1]
    expect(payload).toMatchObject({ renewal: { expiry_date: '2026-03-10' } })
    expect(payload).not.toHaveProperty('expired_at')
    expect(payload.renewal).not.toHaveProperty('billing_timezone')
  })

  it('changes only the billing timezone without resubmitting or moving the selected date', async () => {
    renderEditor()
    const timezone = screen.getByLabelText('Billing timezone')
    expect(timezone).toHaveValue('America/New_York')
    fireEvent.change(timezone, { target: { value: 'Asia/Tokyo' } })
    expect(screen.getByLabelText('Expiration date')).toHaveValue('2026-03-08')
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    const payload = apiBoundary.put.mock.calls[0][1]
    expect(payload.renewal).toEqual({ billing_timezone: 'Asia/Tokyo' })
    expect(payload).not.toHaveProperty('expired_at')
  })

  it('sends an explicit null when the operator clears the expiry date', async () => {
    renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Clear expiration date' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ expiry_date: null })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
  })

  it('keeps a legacy instant intact when the full form is saved unchanged', async () => {
    const { renewal: _renewal, ...legacy } = server
    renderEditor({ ...legacy, expired_at: '2026-03-08T12:34:56Z' })
    expect(screen.getByLabelText('Billing timezone')).toHaveValue('UTC')
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('expired_at')
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it('keeps expiry confirmation untouched when only the cost interval changes', async () => {
    renderEditor()
    fireEvent.click(screen.getByText('Monthly'))
    const interval = await screen.findByRole('option', { name: 'Quarterly' })
    fireEvent.mouseMove(interval)
    fireEvent.click(interval)
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ billing_cycle: 'quarterly' })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it('omits renewal fields after the operator restores the original timezone', async () => {
    renderEditor()
    const timezone = screen.getByLabelText('Billing timezone')
    fireEvent.change(timezone, { target: { value: 'Asia/Tokyo' } })
    fireEvent.change(timezone, { target: { value: 'America/New_York' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it('submits date and timezone clearing together as explicit null values', async () => {
    renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Clear expiration date' }))
    fireEvent.change(screen.getByLabelText('Billing timezone'), { target: { value: '' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ billing_timezone: null, expiry_date: null })
  })

  it('leaves renewal confirmation untouched when only price changes', async () => {
    renderEditor()
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Price' }), { target: { value: '12.50' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1]).toMatchObject({ price: 12.5 })
    expect(apiBoundary.put.mock.calls[0][1]).not.toHaveProperty('renewal')
  })

  it.each(
    CALENDAR_DATES
  )('encodes the selected calendar date $date without browser timezone conversion', async (date) => {
    renderEditor()
    fireEvent.change(screen.getByLabelText('Expiration date'), { target: { value: date.date } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ expiry_date: date.date })
  })

  it('localizes the date and billing timezone controls in Chinese', async () => {
    await i18next.changeLanguage('zh')
    renderEditor()

    expect(screen.getByLabelText('到期日期')).toHaveValue('2026-03-08')
    expect(screen.getByLabelText('账单时区')).toHaveValue('America/New_York')
    expect(screen.getByText('请选择 IANA 时区。服务有效期包含此时区内所选到期日期的整天。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '清除到期日期' })).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: '自动续费跟踪' })).not.toBeChecked()
    expect(screen.getByText('预测到期日')).toBeInTheDocument()
    expect(screen.getByText('请选择到期日期、月付/季付/年付周期和有效的 IANA 账单时区。')).toBeInTheDocument()
    expect(
      screen.getByText(
        '跟踪会在每个到期日期结束后推进预测。关闭后将冻结当前到期日。该估算不代表服务商已续费或付款成功。'
      )
    ).toBeInTheDocument()
  })
})
