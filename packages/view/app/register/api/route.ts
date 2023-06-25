import { cookies } from 'next/headers'
import {NextResponse} from "next/server";
import * as process from "process";
import {ResultUtil} from "@/utils/ResultUtil";

// register
export async function POST(req: Request) {
  const { username, email, password } = await req.json()

  const res = await fetch(`${process.env.BASE_URL}/auth/register`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ username, email, password }),
  })

  const result= await res.json() as ResultUtil<any>

  // if (result.success) {
  //   console.log('req.url', req.url)
  //   return NextResponse.rewrite(new URL('/login', request.url))
  // }

  return NextResponse.json(result)
}
