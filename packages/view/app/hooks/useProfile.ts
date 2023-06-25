import useSWR from "swr";
import {getProfile} from "@/requests/profile/profile";

export function useProfile() {
  const { data, error, isLoading } = useSWR('/profile', getProfile)

  return {
    profile: data?.data,
    isLoading,
    isError: error,
  }
}
