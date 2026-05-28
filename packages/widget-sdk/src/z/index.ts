export type ZodTypeAny = { _kind: string }
export type Infer<T extends ZodTypeAny> = any
