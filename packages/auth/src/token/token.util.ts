import { UnauthorizedException } from "@nestjs/common";

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

function extractBearerTokenFromRawHeaders(rawHeaders: string[]): string {
    const authorizationHeader = rawHeaders.find((header) =>
        header.startsWith('Bearer')
    )
    if (!authorizationHeader) {
        throw new UnauthorizedException('Authorization header not found')
    }
    const token = authorizationHeader.split(' ')[1]
    if (!token) {
        throw new UnauthorizedException('token not found')
    }
    return token
}

export { expireChecker, extractBearerTokenFromRawHeaders }
