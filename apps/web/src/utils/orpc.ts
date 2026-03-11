import { createORPCClient } from '@orpc/client'
import { RPCLink } from '@orpc/client/fetch'
import type { RouterClient } from '@orpc/server'
import { createRouterClient } from '@orpc/server'
import { createTanstackQueryUtils } from '@orpc/tanstack-query'
import { createContext } from '@serverbee/api/context'
import { appRouter } from '@serverbee/api/routers/index'
import { QueryCache, QueryClient } from '@tanstack/react-query'
import { createIsomorphicFn } from '@tanstack/react-start'
import { getRequest } from '@tanstack/react-start/server'
import { toast } from 'sonner'

export const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error, query) => {
      toast.error(`Error: ${error.message}`, {
        action: {
          label: 'retry',
          onClick: query.invalidate
        }
      })
    }
  })
})

const getORPCClient = createIsomorphicFn()
  .server(() =>
    createRouterClient(appRouter, {
      context: async () => {
        return createContext({ req: getRequest() })
      }
    })
  )
  .client((): RouterClient<typeof appRouter> => {
    const link = new RPCLink({
      url: `${window.location.origin}/api/rpc`,
      fetch(url, options) {
        return fetch(url, {
          ...options,
          credentials: 'include'
        })
      }
    })

    return createORPCClient(link)
  })

export const client: RouterClient<typeof appRouter> = getORPCClient()

export const orpc = createTanstackQueryUtils(client)
