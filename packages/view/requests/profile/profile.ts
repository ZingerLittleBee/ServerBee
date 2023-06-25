import instance from "@/requests/instance";
import {ResultUtil} from "@/utils/ResultUtil";
import {ProfileVo} from "@/vo/profile.vo";

export const getProfile = async (url: string) => {
  const { data } = await instance.get<ResultUtil<ProfileVo>>(url)
  return data
}
