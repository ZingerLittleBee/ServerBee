import { createEnv } from '@t3-oss/env-core'

export const env = createEnv({
  clientPrefix: 'VITE_',
  client: {},
  runtimeEnv: (import.meta as any).env,
  emptyStringAsUndefined: true
})
