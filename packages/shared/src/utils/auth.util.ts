import { Request } from 'express'
import { kAccessToken, kRefreshToken } from '@/const/auth.const'

const extractAccessTokenFromCookie = (request: Request): string | undefined => {
    return request.cookies[kAccessToken]
}

const extractRefreshTokenFromCookie = (
    request: Request
): string | undefined => {
    return request.cookies[kRefreshToken]
}

const extractTokenFromHeaderAuth = (request: Request): string | undefined => {
    const [type, token] = request.headers?.authorization?.split(' ') ?? []
    return type === 'Bearer' ? token : undefined
}

const extractAccessToken = (request: Request): string | undefined => {
    return (
        extractAccessTokenFromCookie(request) ||
        extractTokenFromHeaderAuth(request)
    )
}

export {
    extractAccessTokenFromCookie,
    extractRefreshTokenFromCookie,
    extractTokenFromHeaderAuth,
    extractAccessToken
}
