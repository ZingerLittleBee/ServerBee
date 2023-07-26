import { addClientUrl } from "@/requests/endpoint"
import instance from "@/requests/instance"
import { ClientVo, Result } from "@server-octopus/types"

export const addClientRequest = async (name: string) => {
  const { data } = await instance.post<Result<ClientVo>>(addClientUrl, {
    name,
  })
  return data
}
