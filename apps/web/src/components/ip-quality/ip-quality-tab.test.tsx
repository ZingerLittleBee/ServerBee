import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CAP_EXEC, CAP_IP_QUALITY } from '@/lib/capabilities'
import en from '@/locales/en/ip-quality.json'
import { IpQualityTab } from './ip-quality-tab'

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
})
