import type { OnMount } from '@monaco-editor/react'
import { Loader2 } from 'lucide-react'
import { lazy, Suspense, useCallback, useRef } from 'react'
import { useTheme } from '@/components/theme-provider'

const MonacoEditor = lazy(() => import('@monaco-editor/react'))

interface FileEditorProps {
  content: string
  language: string
  onSave?: (content: string) => void
  readOnly?: boolean
}

function resolveTheme(theme: string): string {
  if (theme === 'dark') {
    return 'vs-dark'
  }
  if (theme === 'light') {
    return 'light'
  }
  // system: check media query
  if (typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    return 'vs-dark'
  }
  return 'light'
}

export function FileEditor({ content, language, readOnly = false, onSave }: FileEditorProps) {
  const { theme } = useTheme()
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null)

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor

      if (onSave) {
        // biome-ignore lint/suspicious/noBitwiseOperators: intentional Monaco KeyMod bitmask
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
          const value = editor.getValue()
          onSave(value)
        })
      }
    },
    [onSave]
  )

  return (
    <Suspense
      fallback={
        <div className="flex h-full items-center justify-center">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      }
    >
      <MonacoEditor
        defaultValue={content}
        height="100%"
        language={language}
        onMount={handleMount}
        options={{
          readOnly,
          minimap: { enabled: true },
          wordWrap: 'on',
          fontSize: 14,
          tabSize: 2,
          scrollBeyondLastLine: false,
          automaticLayout: true
        }}
        theme={resolveTheme(theme)}
      />
    </Suspense>
  )
}
