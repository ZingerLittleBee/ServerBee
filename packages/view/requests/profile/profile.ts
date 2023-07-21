import instance from "@/requests/instance"
import { ProfileVo, Result } from "@server-octopus/types"

export const getProfile = async (url: string) => {
  const { data } = await instance.get<Result<ProfileVo>>(url)
  return data
}
