import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { LoginRequest, MeResponse } from '@/lib/api-schema'

export function useAuth() {
  const queryClient = useQueryClient()

  const {
    data: user,
    isLoading,
    isError
  } = useQuery<MeResponse | null>({
    queryKey: ['auth', 'me'],
    queryFn: async () => {
      try {
        return await api.get<MeResponse>('/api/auth/me')
      } catch {
        return null
      }
    },
    retry: false,
    staleTime: 60_000
  })

  const loginMutation = useMutation({
    mutationFn: (payload: LoginRequest) => api.post<MeResponse>('/api/auth/login', payload),
    onSuccess: (data) => {
      queryClient.setQueryData(['auth', 'me'], data)
    }
  })

  const logoutMutation = useMutation({
    mutationFn: () => api.post('/api/auth/logout'),
    onSuccess: () => {
      queryClient.setQueryData(['auth', 'me'], null)
      queryClient.clear()
    }
  })

  return {
    user: user ?? null,
    isLoading,
    isAuthenticated: !(isLoading || isError) && user != null,
    login: loginMutation.mutateAsync,
    logout: logoutMutation.mutateAsync,
    loginError: loginMutation.error,
    isLoggingIn: loginMutation.isPending
  }
}
