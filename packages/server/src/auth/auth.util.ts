import { Request } from 'express'
import { kAccessToken, kRefreshToken } from './auth.const'

const extractAccessTokenFromHeader = (request: Request): string | undefined => {
    return request.cookies[kAccessToken]
}

const extractRefreshTokenFromHeader = (
    request: Request
): string | undefined => {
    return request.cookies[kRefreshToken]
}

export { extractAccessTokenFromHeader, extractRefreshTokenFromHeader }
