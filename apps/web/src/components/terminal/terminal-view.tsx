import '@xterm/xterm/css/xterm.css'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import { useCallback, useEffect, useRef } from 'react'

interface TerminalViewProps {
  onData: (data: string) => void
  onResize: (rows: number, cols: number) => void
  writeRef: React.MutableRefObject<((data: string) => void) | null>
}

export function TerminalView({ onData, onResize, writeRef }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)

  const handleResize = useCallback(() => {
    if (fitAddonRef.current && terminalRef.current) {
      fitAddonRef.current.fit()
      onResize(terminalRef.current.rows, terminalRef.current.cols)
    }
  }, [onResize])

  useEffect(() => {
    if (!containerRef.current) {
      return
    }

    const terminal = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      theme: {
        background: '#1a1b26',
        foreground: '#a9b1d6',
        cursor: '#c0caf5',
        selectionBackground: '#33467C',
        black: '#32344a',
        red: '#f7768e',
        green: '#9ece6a',
        yellow: '#e0af68',
        blue: '#7aa2f7',
        magenta: '#ad8ee6',
        cyan: '#449dab',
        white: '#787c99',
        brightBlack: '#444b6a',
        brightRed: '#ff7a93',
        brightGreen: '#b9f27c',
        brightYellow: '#ff9e64',
        brightBlue: '#7da6ff',
        brightMagenta: '#bb9af7',
        brightCyan: '#0db9d7',
        brightWhite: '#acb0d0'
      }
    })

    const fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)
    terminal.loadAddon(new WebLinksAddon())

    terminal.open(containerRef.current)
    fitAddon.fit()

    terminalRef.current = terminal
    fitAddonRef.current = fitAddon

    // Forward terminal input to parent
    terminal.onData(onData)

    // Expose write function
    writeRef.current = (data: string) => {
      terminal.write(data)
    }

    // Report initial size
    onResize(terminal.rows, terminal.cols)

    // Handle container resize
    const observer = new ResizeObserver(() => {
      handleResize()
    })
    observer.observe(containerRef.current)

    return () => {
      observer.disconnect()
      terminal.dispose()
      terminalRef.current = null
      fitAddonRef.current = null
      writeRef.current = null
    }
  }, [onData, onResize, writeRef, handleResize])

  return <div className="h-full w-full overflow-hidden rounded-md border bg-[#1a1b26] p-1" ref={containerRef} />
}
