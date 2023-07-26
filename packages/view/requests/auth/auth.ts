import { loginUrl, logoutUrl, registerUrl } from "@/requests/endpoint"
import instance from "@/requests/instance"
import { Result } from "@server-octopus/types"

import { toast } from "@/components/ui/use-toast"





export const login = async (
  password: string,
  email?: string,
  username?: string
) => {
  if (!email && !username) {
    toast({
      title: "Email or Username is required",
    })
    return
  }
  const { data } = await instance.post<Result>(loginUrl, {
    email,
    username,
    password,
  })
  return data
}

export const register = async (
  email: string,
  username: string,
  password: string
) => {
  const { data } = await instance.post<Result>(registerUrl, {
    email,
    username,
    password,
  })
  return data
}

export const logout = async () => {
  const { data } = await instance.post<Result>(logoutUrl)
  return data.success
}
