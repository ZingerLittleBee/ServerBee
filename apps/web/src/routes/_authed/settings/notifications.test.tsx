import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Notification } from '@/lib/api-schema'
import { EmailFormFields, NotificationChannelsSection } from './notification-channel-section'

const SMTP_HOST_RE = /smtp_host/i
const SMTP_PORT_RE = /smtp_port/i
const SMTP_USERNAME_RE = /username/i
const SMTP_PASSWORD_RE = /password/i
const mockFetch = vi.fn<typeof fetch>()
const getAnimationsDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'getAnimations')

beforeAll(() => {
  // jsdom has no Web Animations API; the real ScrollArea uses it during cleanup.
  Object.defineProperty(Element.prototype, 'getAnimations', { configurable: true, value: () => [] })
})

afterAll(() => {
  if (getAnimationsDescriptor) {
    Object.defineProperty(Element.prototype, 'getAnimations', getAnimationsDescriptor)
  } else {
    Reflect.deleteProperty(Element.prototype, 'getAnimations')
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (options && typeof options === 'object' && 'address' in options) {
        return `${key}:${String(options.address)}`
      }
      return key
    }
  })
}))

beforeEach(() => {
  mockFetch.mockReset()
  mockFetch.mockImplementation(() => Promise.resolve(Response.json({ data: null })))
  vi.stubGlobal('fetch', mockFetch)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function noop() {
  // intentionally empty
}

function renderChannels(notifications: Notification[] = []) {
  const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={queryClient}>
      <NotificationChannelsSection isLoading={false} notifications={notifications} />
    </QueryClientProvider>
  )
}

async function openEmailChannel() {
  fireEvent.click(screen.getByRole('button', { name: 'common:add' }))
  fireEvent.click(await screen.findByRole('combobox'))
  const emailOption = await screen.findByRole('option', { name: 'notifications.type_email' })
  fireEvent.mouseMove(emailOption)
  fireEvent.click(emailOption)
  await screen.findByPlaceholderText('notifications.from_address')
}

function addRecipient(address: string) {
  fireEvent.change(screen.getByPlaceholderText('notifications.recipient_placeholder'), {
    target: { value: address }
  })
  fireEvent.click(screen.getByRole('button', { name: 'notifications.add_recipient' }))
}

describe('email channel submission', () => {
  it('creates a channel with trimmed, unique recipients in entry order', async () => {
    renderChannels()
    await openEmailChannel()
    fireEvent.change(screen.getByPlaceholderText('notifications.channel_name'), {
      target: { value: '  Team alerts  ' }
    })
    fireEvent.change(screen.getByPlaceholderText('notifications.from_address'), {
      target: { value: 'alerts@example.com' }
    })
    addRecipient(' z@example.com ')
    addRecipient('a@example.com')
    addRecipient('z@example.com')
    addRecipient('m@example.com')
    fireEvent.click(screen.getByRole('button', { name: 'common:create' }))

    await waitFor(() =>
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/notifications',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            name: 'Team alerts',
            notify_type: 'email',
            config_json: { from: 'alerts@example.com', to: ['z@example.com', 'a@example.com', 'm@example.com'] },
            enabled: true
          })
        })
      )
    )
    expect(mockFetch).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('updates an existing email channel without reordering recipients or replacing a blank sender', async () => {
    renderChannels([
      {
        id: 'channel-email',
        name: 'Existing alerts',
        notify_type: 'email',
        config_json: JSON.stringify({ to: ['z@example.com', 'a@example.com', 'm@example.com'] }),
        enabled: false,
        created_at: '2026-01-01T00:00:00Z'
      }
    ])
    fireEvent.click(screen.getByRole('button', { name: 'common:a11y.edit_notification' }))
    const fromInput = await screen.findByPlaceholderText('notifications.from_address')
    expect(fromInput).toHaveValue('')
    expect(fromInput).toBeRequired()
    fireEvent.click(screen.getByRole('button', { name: 'notifications.remove_recipient_aria:a@example.com' }))
    addRecipient('a@example.com')

    // Dispatch submit directly to cover the existing blank-sender serialization;
    // the required attribute above remains the browser's submission constraint.
    const form = fromInput.closest('form')
    if (!form) {
      throw new Error('email channel form is missing')
    }
    fireEvent.submit(form)

    await waitFor(() =>
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/notifications/channel-email',
        expect.objectContaining({
          method: 'PUT',
          body: JSON.stringify({
            name: 'Existing alerts',
            notify_type: 'email',
            config_json: { from: '', to: ['z@example.com', 'm@example.com', 'a@example.com'] },
            enabled: false
          })
        })
      )
    )
    expect(mockFetch).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('does not send a request when recipients are missing or rejected', async () => {
    renderChannels()
    await openEmailChannel()
    fireEvent.change(screen.getByPlaceholderText('notifications.channel_name'), {
      target: { value: 'Team alerts' }
    })
    const fromInput = screen.getByPlaceholderText('notifications.from_address')
    fireEvent.change(fromInput, { target: { value: 'alerts@example.com' } })
    addRecipient('invalid@example')
    const form = fromInput.closest('form')
    if (!form) {
      throw new Error('email channel form is missing')
    }
    await act(async () => {
      fireEvent.submit(form)
      await Promise.resolve()
    })

    expect(mockFetch).not.toHaveBeenCalled()
    expect(screen.getByRole('dialog')).toBeDefined()
    expect(screen.queryByRole('button', { name: 'notifications.remove_recipient_aria:invalid@example' })).toBeNull()
  })
})

describe('EmailFormFields', () => {
  it('does not render any SMTP fields', () => {
    render(
      <EmailFormFields
        from=""
        onAddRecipient={noop}
        onFromChange={noop}
        onRemoveRecipient={noop}
        onToInputChange={noop}
        toAddresses={[]}
        toInput=""
      />
    )

    // SMTP fields from the legacy email schema must not appear anywhere in the rendered form.
    expect(screen.queryByPlaceholderText(SMTP_HOST_RE)).toBeNull()
    expect(screen.queryByPlaceholderText(SMTP_PORT_RE)).toBeNull()
    expect(screen.queryByPlaceholderText(SMTP_USERNAME_RE)).toBeNull()
    expect(screen.queryByPlaceholderText(SMTP_PASSWORD_RE)).toBeNull()
    // And there should be no password-type input (used by the legacy SMTP password field).
    const passwordInputs = document.querySelectorAll('input[type="password"]')
    expect(passwordInputs.length).toBe(0)
  })

  it('renders the from input, recipient input, and recipient tags', () => {
    render(
      <EmailFormFields
        from="alerts@example.com"
        onAddRecipient={noop}
        onFromChange={noop}
        onRemoveRecipient={noop}
        onToInputChange={noop}
        toAddresses={['ops@example.com']}
        toInput=""
      />
    )

    // The "from" address shows up as the current value of an input.
    const fromInput = screen.getByDisplayValue('alerts@example.com')
    expect(fromInput).toBeDefined()
    expect((fromInput as HTMLInputElement).type).toBe('email')

    // Placeholder for the from input and recipient input are rendered via translation keys.
    expect(screen.getByPlaceholderText('notifications.from_address')).toBeDefined()
    expect(screen.getByPlaceholderText('notifications.recipient_placeholder')).toBeDefined()

    // The existing recipient renders as a tag with a remove button.
    expect(screen.getByText('ops@example.com')).toBeDefined()
    expect(screen.getByLabelText('notifications.remove_recipient_aria:ops@example.com')).toBeDefined()
  })
})
