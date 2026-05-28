import type { ComponentType, ReactNode } from 'react'
import type { Infer, ZodTypeAny } from './z'

export interface ActionContext {
  apiMutation: <Req = unknown, Res = unknown>(method: string, path: string, body?: Req) => Promise<Res>
}

export interface ActionDefinition {
  confirm?: { title: string; body?: string }
  icon?: string
  id: string
  label: string
  run: (ctx: ActionContext) => Promise<void>
}

export interface ActionsHelper {
  render: (id: string) => ReactNode
}

export interface WidgetComponentProps<TConfig> {
  actions: ActionsHelper
  config: TConfig
  isEditing: boolean
  size: { w: number; h: number }
}

export interface DefineWidgetInput<TSchema extends ZodTypeAny> {
  actions?: ActionDefinition[]
  component: ComponentType<WidgetComponentProps<Infer<TSchema>>>
  configSchema: TSchema
}

export interface WidgetModule<TConfig = unknown> {
  __brand: 'WidgetModule'
  actions: ActionDefinition[]
  component: ComponentType<WidgetComponentProps<TConfig>>
  configSchema: ZodTypeAny
}

export function defineWidget<TSchema extends ZodTypeAny>(
  input: DefineWidgetInput<TSchema>
): WidgetModule<Infer<TSchema>> {
  if (!input || typeof input.component !== 'function') {
    throw new Error('defineWidget: component is required')
  }
  return {
    __brand: 'WidgetModule',
    configSchema: input.configSchema,
    component: input.component as any,
    actions: input.actions ?? []
  }
}
