import { useState } from 'react'
import type { ActionDefinition } from '../define-widget'

interface Props {
  action: ActionDefinition
  onRun: () => Promise<void>
}

export function ActionButton({ action, onRun }: Props) {
  const [pending, setPending] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const trigger = async () => {
    if (action.confirm && !confirming) {
      setConfirming(true)
      return
    }
    setConfirming(false)
    setPending(true)
    try {
      await onRun()
    } finally {
      setPending(false)
    }
  }
  return (
    <button disabled={pending} onClick={trigger} type="button">
      {confirming ? `Confirm: ${action.label}` : action.label}
      {pending ? '…' : ''}
    </button>
  )
}
