"use client"

import {useProfile} from "@/app/hooks/useProfile";

export default () => {
  const { profile, isLoading, isError  } = useProfile()

  return (
    <div>
      <h1>Dashboard</h1>
      {
        isLoading ? <div>Loading...</div> : <div>{ profile?.user.userId }</div>
      }
    </div>
  )
}
