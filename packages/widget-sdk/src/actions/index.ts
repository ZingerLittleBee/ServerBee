import { createElement, type ReactNode } from 'react'
import type { ActionContext, ActionDefinition, ActionsHelper } from '../define-widget'
import { ActionButton } from './action-button'

async function apiMutation<Req = unknown, Res = unknown>(method: string, path: string, body?: Req): Promise<Res> {
  const res = await fetch(path, {
    method,
    credentials: 'include',
    headers: body ? { 'content-type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined
  })
  if (!res.ok) {
    throw new Error(`${method} ${path}: ${res.status}`)
  }
  const json: unknown = await res.json()
  if (json && typeof json === 'object' && 'data' in json) {
    return (json as { data: Res }).data
  }
  return json as Res
}

export function createActionsHelper(actions: ActionDefinition[]): ActionsHelper {
  const ctx: ActionContext = { apiMutation }
  return {
    render(id: string): ReactNode {
      const action = actions.find((a) => a.id === id)
      if (!action) {
        return null
      }
      return createElement(ActionButton, { key: id, action, onRun: () => action.run(ctx) })
    }
  }
}

export type { ActionContext, ActionDefinition, ActionsHelper } from '../define-widget'
