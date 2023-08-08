type TokenGroup = {
    accessToken: string
    refreshToken: string
}

type TokenExpiration = {
    accessExpiration: number
    refreshExpiration: number
}

type TokenInfo = {
    token: string
    expiration: number
}

type TokenInfoGroup = {
    access: TokenInfo
    refresh: TokenInfo
}

type TokenPayload = {
    userId: string
    clientId: string
}


export {
    TokenExpiration, TokenGroup, TokenInfo,
    TokenInfoGroup, TokenPayload
}

