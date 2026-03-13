import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

interface User {
  must_change_password: boolean
  role: string
  user_id: string
  username: string
}

interface LoginPayload {
  password: string
  totp_code?: string
  username: string
}

export function useAuth() {
  const queryClient = useQueryClient()

  const {
    data: user,
    isLoading,
    isError
  } = useQuery<User | null>({
    queryKey: ['auth', 'me'],
    queryFn: async () => {
      try {
        return await api.get<User>('/api/auth/me')
      } catch {
        return null
      }
    },
    retry: false,
    staleTime: 60_000
  })

  const loginMutation = useMutation({
    mutationFn: (payload: LoginPayload) => api.post<User>('/api/auth/login', payload),
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
