import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { CAP_EXEC, CAP_IP_QUALITY } from '@/lib/capabilities'
import en from '@/locales/en/ip-quality.json'
import { IpQualityTab } from './ip-quality-tab'

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

describe('IpQualityTab', () => {
  it('gates manual checks on the Agent report and reacts to capability revocation', () => {
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Number.POSITIVE_INFINITY } } })
    client.setQueryData(['ip-quality', 'services'], [])
    client.setQueryData(['ip-quality', 'events', 'srv-1'], [])
    client.setQueryData(['ip-quality', 'servers', 'srv-1'], {
      server_id: 'srv-1',
      ip_quality: null,
      unlock_results: []
    })

    const { rerender, unmount } = render(
      <IpQualityTab agentLocalCapabilities={null} serverId="srv-1" serverName="Server 1" />,
      {
        wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
      }
    )
    const checkNow = screen.getByRole('button', { name: en.check_now })
    expect(checkNow).toBeDisabled()

    rerender(<IpQualityTab agentLocalCapabilities={CAP_EXEC} serverId="srv-1" serverName="Server 1" />)
    expect(checkNow).toBeDisabled()

    rerender(<IpQualityTab agentLocalCapabilities={CAP_IP_QUALITY} serverId="srv-1" serverName="Server 1" />)
    expect(checkNow).toBeEnabled()

    rerender(<IpQualityTab agentLocalCapabilities={CAP_EXEC} serverId="srv-1" serverName="Server 1" />)
    expect(checkNow).toBeDisabled()

    unmount()
    client.clear()
  })

  it('keeps capability guidance in a dismissible dialog instead of the page', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Number.POSITIVE_INFINITY } } })
    client.setQueryData(['ip-quality', 'services'], [])
    client.setQueryData(['ip-quality', 'events', 'srv-1'], [])
    client.setQueryData(['ip-quality', 'servers', 'srv-1'], {
      server_id: 'srv-1',
      ip_quality: null,
      unlock_results: []
    })

    const { unmount } = render(<IpQualityTab agentLocalCapabilities={0} serverId="srv-1" serverName="Server 1" />, {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
    })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()

    const trigger = screen.getByRole('button', { name: en.cap_enable_action })
    fireEvent.click(trigger)
    const dialog = await screen.findByRole('dialog', { name: en.cap_enable_dialog_title })
    expect(dialog).toBeVisible()

    fireEvent.keyDown(dialog, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    await waitFor(() => expect(trigger).toHaveFocus())

    unmount()
    client.clear()
  })
})
