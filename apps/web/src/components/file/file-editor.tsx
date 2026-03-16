import type { OnMount } from '@monaco-editor/react'
import { Loader2 } from 'lucide-react'
import type { MutableRefObject } from 'react'
import { lazy, Suspense, useCallback, useRef } from 'react'
import { useTheme } from '@/components/theme-provider'

const MonacoEditor = lazy(() => import('@monaco-editor/react'))

export type MonacoEditorInstance = Parameters<OnMount>[0]

interface FileEditorProps {
  content: string
  editorRef?: MutableRefObject<MonacoEditorInstance | null>
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

export function FileEditor({
  content,
  editorRef: externalEditorRef,
  language,
  readOnly = false,
  onSave
}: FileEditorProps) {
  const { theme } = useTheme()
  const internalEditorRef = useRef<MonacoEditorInstance | null>(null)

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      internalEditorRef.current = editor
      if (externalEditorRef) {
        externalEditorRef.current = editor
      }

      if (onSave) {
        // biome-ignore lint/suspicious/noBitwiseOperators: intentional Monaco KeyMod bitmask
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
          const value = editor.getValue()
          onSave(value)
        })
      }
    },
    [onSave, externalEditorRef]
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
