import { useState } from 'react'
import type { ActionDefinition } from '../define-widget'
import { getRuntime } from '../runtime-context'

interface Props {
  action: ActionDefinition
  onRun: () => Promise<void>
}

async function askConfirm(action: ActionDefinition): Promise<boolean> {
  if (!action.confirm) {
    return true
  }
  try {
    const rt = getRuntime()
    if (rt.requestConfirm) {
      return await rt.requestConfirm(action.confirm)
    }
  } catch {
    // runtime not installed — fall back to window.confirm
  }
  if (typeof window !== 'undefined' && typeof window.confirm === 'function') {
    const body = action.confirm.body ? `\n\n${action.confirm.body}` : ''
    // biome-ignore lint/suspicious/noAlert: deliberate fallback when host runtime has not wired requestConfirm
    return window.confirm(`${action.confirm.title}${body}`)
  }
  return true
}

function notify(type: 'success' | 'error', message: string): void {
  try {
    const rt = getRuntime()
    if (rt.notify) {
      rt.notify({ type, message })
      return
    }
  } catch {
    // runtime not installed
  }
  if (type === 'error') {
    console.error(`[widget-action] ${message}`)
  } else {
    console.info(`[widget-action] ${message}`)
  }
}

// NOTE: server-side widget-action audit log is intentionally deferred. The
// backend endpoint (POST /api/widget-actions/audit) does not exist yet; once
// it lands, we'll fire a best-effort POST from here on success. Until then we
// log via console.info with the `[widget-action]` prefix.
export function ActionButton({ action, onRun }: Props) {
  const [pending, setPending] = useState(false)

  const trigger = async () => {
    const ok = await askConfirm(action)
    if (!ok) {
      return
    }
    setPending(true)
    try {
      await onRun()
      notify('success', `${action.label}: done`)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      notify('error', `${action.label}: ${msg}`)
    } finally {
      setPending(false)
    }
  }

  return (
    <button
      className="rounded-md border px-3 py-1 text-sm hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
      disabled={pending}
      onClick={trigger}
      type="button"
    >
      {pending ? `${action.label}…` : action.label}
    </button>
  )
}
