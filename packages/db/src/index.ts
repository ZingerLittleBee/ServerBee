import { createClient } from '@libsql/client'
import { env } from '@serverbee/env/server'
import { drizzle } from 'drizzle-orm/libsql'

import { account, accountRelations, session, sessionRelations, todo, user, userRelations, verification } from './schema'

const client = createClient({
  url: env.DATABASE_URL
})

export const db = drizzle({
  client,
  schema: { account, accountRelations, session, sessionRelations, todo, user, userRelations, verification }
})
