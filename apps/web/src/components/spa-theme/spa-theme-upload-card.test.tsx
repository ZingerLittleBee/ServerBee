import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import '@/lib/i18n'
import { SpaThemeUploadCard } from './spa-theme-upload-card'

const DRAG_PROMPT_RE = /Drag .sbtheme/

function wrap(node: ReactNode) {
  return <QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>
}

describe('SpaThemeUploadCard', () => {
  it('renders the drag prompt', () => {
    render(wrap(<SpaThemeUploadCard />))
    expect(screen.getByText(DRAG_PROMPT_RE)).toBeInTheDocument()
  })

  it('submits a multipart POST when a file is selected', async () => {
    const spy = vi.spyOn(global, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: { uuid: 'u', manifest: {}, size_bytes: 1, preview_url: null, is_upgrade_of: null }
        }),
        { status: 200 }
      )
    )
    render(wrap(<SpaThemeUploadCard />))
    const input = document.querySelector('input[type=file]') as HTMLInputElement
    const file = new File(['x'], 'a.sbtheme', { type: 'application/zip' })
    fireEvent.change(input, { target: { files: [file] } })
    await vi.waitFor(() => expect(spy).toHaveBeenCalled())
    expect(spy.mock.calls[0][1]?.method).toBe('POST')
    spy.mockRestore()
  })
})
