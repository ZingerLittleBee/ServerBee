import { NextResponse, type NextRequest } from "next/server"

import { isAuthenticated } from "@/lib/auth"
import { kDashboardRoute, kLoginRoute, kSettingsRoute } from "@/lib/route"





export async function middleware(request: NextRequest) {
  const cookie = request.cookies.toString()
  const url = request.nextUrl.clone()

  const isAuth = await isAuthenticated(cookie)

  if (request.nextUrl.pathname.startsWith(kLoginRoute) && isAuth) {
    return NextResponse.redirect(new URL(kDashboardRoute, request.url))
  }

  if (!cookie || !isAuth) {
    url.pathname = kSettingsRoute
    return NextResponse.rewrite(url)
  }
  return NextResponse.next()
}

// See "Matching Paths" below to learn more
export const config = {
  matcher: ['/login', '/settings'],
}
