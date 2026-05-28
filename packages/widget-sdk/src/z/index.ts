import { color, duration, metricPath, serverId } from './extensions'
import { type Infer, ZodSchema, type ZodTypeAny, z as zPrimitives } from './primitives'

export const z = Object.assign(zPrimitives, { serverId, metricPath, color, duration })
export { ZodSchema, type ZodTypeAny, type Infer }
export { ZError } from './validate'
