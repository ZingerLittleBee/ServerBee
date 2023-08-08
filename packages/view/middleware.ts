import { NextResponse, type NextRequest } from "next/server"

import { isAuthenticated } from "@/lib/auth"
import { kDashboardRoute, kLoginRoute } from "@/lib/route"





export async function middleware(request: NextRequest) {
  const cookie = request.cookies.toString()
  const url = request.nextUrl.clone()

  const isAuth = await isAuthenticated(cookie)

  if (request.nextUrl.pathname.startsWith(kLoginRoute)) {
    return isAuth
      ? NextResponse.redirect(new URL(kDashboardRoute, request.url))
      : NextResponse.next()
  }

  if (!cookie || !isAuth) {
    url.pathname = kLoginRoute
    return NextResponse.rewrite(url)
  }
  return NextResponse.next()
}

// See "Matching Paths" below to learn more
export const config = {
  matcher: ['/login', '/settings'],
}
