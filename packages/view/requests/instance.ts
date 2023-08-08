import { Result } from "@server-octopus/types"
import axios from "axios"

import { toast } from "@/components/ui/use-toast"





const instance = axios.create({
  baseURL: 'http://localhost:3000/api',
  withCredentials: true,
  headers: {
    'Content-Type': 'application/json'
  }
});

instance.interceptors.response.use(function (response) {
  if (response.data) {
    const result = response.data as Result

    if (!result.success) {
      toast(
        {
          title: result.message,
        }
      )
    }
  }

  return response;
}, function (error) {
  return Promise.reject(error);
});

export default instance
