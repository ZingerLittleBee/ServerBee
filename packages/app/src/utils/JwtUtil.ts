import { UnauthorizedException } from '@nestjs/common'

/**
 * @description check if the token is expired
 * @param {string} exp
 * @return {boolean} true if expired
 */
export function expireChecker(exp: number): void {
    if (Date.now() >= exp * 1000) {
        throw new UnauthorizedException(`token expired`)
    }
}

export const kClientAccessExpireTime = 'JWT_CLIENT_ACCESS_EXPIRATION_TIME'
