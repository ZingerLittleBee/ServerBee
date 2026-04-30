import type { CSSProperties } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

interface ThemePreviewProps {
  dark: boolean
  vars: Record<string, string>
}

function cssVariableKey(key: string): `--${string}` {
  return key.startsWith('--') ? `--${key.slice(2)}` : `--${key}`
}

export function ThemePreview({ dark, vars }: ThemePreviewProps) {
  const style: CSSProperties & Record<`--${string}`, string> = {
    background: 'var(--background)',
    color: 'var(--foreground)'
  }

  for (const [key, value] of Object.entries(vars)) {
    style[cssVariableKey(key)] = value
  }

  return (
    <div className={`flex-1 p-6 ${dark ? 'dark' : ''}`} data-testid="theme-preview" data-theme-preview style={style}>
      <div className="rounded-lg border p-4" style={{ background: 'var(--card)', borderColor: 'var(--border)' }}>
        <h3 className="mb-2 font-semibold">Sample Card</h3>
        <p className="mb-3 text-sm" style={{ color: 'var(--muted-foreground)' }}>
          Preview of typography, buttons, and inputs.
        </p>
        <div className="mb-3 flex flex-wrap gap-2">
          <Button type="button">Primary</Button>
          <Button type="button" variant="secondary">
            Secondary
          </Button>
          <Button type="button" variant="destructive">
            Destructive
          </Button>
        </div>
        <Input className="mb-3" placeholder="Input field" />
        <div className="flex flex-wrap gap-2">
          <Badge>Default</Badge>
          <Badge variant="secondary">Secondary</Badge>
          <Badge variant="outline">Outline</Badge>
        </div>
      </div>

      <div className="mt-4 grid grid-cols-5 gap-2">
        {[1, 2, 3, 4, 5].map((index) => (
          <div className="h-16 rounded" key={index} style={{ background: `var(--chart-${index})` }} />
        ))}
      </div>
    </div>
  )
}
