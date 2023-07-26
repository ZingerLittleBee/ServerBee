import { profileUrl } from "@/requests/endpoint"
import { getProfile } from "@/requests/profile/profile"
import useSWR from "swr"

export function useProfile() {
  const { data, error, isLoading } = useSWR(profileUrl, getProfile)

  return {
    profile: data?.data,
    isLoading,
    isError: error,
  }
}
