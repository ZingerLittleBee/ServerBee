import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

const SMTP_HOST_RE = /smtp_host/i
const SMTP_PORT_RE = /smtp_port/i
const SMTP_USERNAME_RE = /username/i
const SMTP_PASSWORD_RE = /password/i

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

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <button type="button" {...props}>
      {children}
    </button>
  )
}))

vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />
}))

vi.mock('@/components/ui/label', () => ({
  // Tests don't care about label/control association; a span keeps Biome happy.
  Label: ({ children, ...props }: { children?: ReactNode } & Record<string, unknown>) => (
    <span {...props}>{children}</span>
  )
}))

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => config
}))

const { EmailFormFields, buildEmailPayload } = await import('./notifications')

function noop() {
  // intentionally empty
}

describe('buildEmailPayload', () => {
  it('wraps a single recipient as a string array', () => {
    const payload = buildEmailPayload('alerts@example.com', ['ops@example.com'])
    expect(payload).toEqual({ from: 'alerts@example.com', to: ['ops@example.com'] })
  })

  it('preserves multiple recipients in order', () => {
    const payload = buildEmailPayload('alerts@example.com', ['a@x.com', 'b@y.com', 'c@z.com'])
    expect(payload.to).toEqual(['a@x.com', 'b@y.com', 'c@z.com'])
  })

  it('allows an empty from (validation happens at submit time)', () => {
    const payload = buildEmailPayload('', ['ops@example.com'])
    expect(payload.from).toBe('')
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
