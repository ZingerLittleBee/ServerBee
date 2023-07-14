import { UnauthorizedException } from '@nestjs/common'

/**
 * @description check if the token is expired
 * @param {string} exp
 * @return {boolean} true if expired
 */
function expireChecker(exp: number): void {
    if (Date.now() >= exp * 1000) {
        throw new UnauthorizedException(`token expired`)
    }
}

export { expireChecker }
