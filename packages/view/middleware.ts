import { NextResponse, type NextRequest } from "next/server"

import { isAuthenticated } from "@/lib/auth"





export async function middleware(request: NextRequest) {
  const cookie = request.cookies.toString()
  const url = request.nextUrl.clone()

  if (!cookie || !(await isAuthenticated(cookie))) {
    url.pathname = "/login"
    return NextResponse.rewrite(url)
  }
  return NextResponse.next()
}

// See "Matching Paths" below to learn more
export const config = {
  matcher: "/settings",
}
