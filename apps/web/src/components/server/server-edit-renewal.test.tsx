import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import i18next from 'i18next'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerResponse } from '@/lib/api-schema'
import { ServerEditDialog } from './server-edit-dialog'

const apiBoundary = vi.hoisted(() => ({ get: vi.fn(), put: vi.fn() }))
const MONTH_LABEL = /month/i
const MARCH_TENTH = /March 10.*2026/
const YEAR_LABEL = /year/i
const CALENDAR_DATES = [
  { date: '2026-01-31', label: /January 31.*2026/, month: '0', year: '2026' },
  { date: '2024-02-29', label: /February 29.*2024/, month: '1', year: '2024' }
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
  return render(
    <QueryClientProvider client={client}>
      <ServerEditDialog onClose={vi.fn()} open server={value} />
    </QueryClientProvider>
  )
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
  it('displays the server-selected local expiry date instead of its UTC day', () => {
    renderEditor()

    expect(screen.getByRole('button', { name: 'Expiration date' })).toHaveTextContent('2026-03-08')
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
    fireEvent.click(screen.getByRole('button', { name: 'Expiration date' }))
    fireEvent.change(await screen.findByRole('combobox', { name: MONTH_LABEL }), { target: { value: '2' } })
    fireEvent.click(screen.getByRole('button', { name: MARCH_TENTH }))
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
    expect(screen.getByRole('button', { name: 'Expiration date' })).toHaveTextContent('2026-03-08')
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
    fireEvent.click(screen.getByRole('button', { name: 'Expiration date' }))
    fireEvent.change(await screen.findByRole('combobox', { name: MONTH_LABEL }), { target: { value: date.month } })
    fireEvent.change(screen.getByRole('combobox', { name: YEAR_LABEL }), { target: { value: date.year } })
    fireEvent.click(screen.getByRole('button', { name: date.label }))
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(apiBoundary.put).toHaveBeenCalled())
    expect(apiBoundary.put.mock.calls[0][1].renewal).toEqual({ expiry_date: date.date })
  })

  it('localizes the date and billing timezone controls in Chinese', async () => {
    await i18next.changeLanguage('zh')
    renderEditor()

    expect(screen.getByRole('button', { name: '到期日期' })).toHaveTextContent('2026-03-08')
    expect(screen.getByLabelText('账单时区')).toHaveValue('America/New_York')
    expect(screen.getByText('请选择 IANA 时区。服务有效期包含此时区内所选到期日期的整天。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '清除到期日期' })).toBeInTheDocument()
  })
})
