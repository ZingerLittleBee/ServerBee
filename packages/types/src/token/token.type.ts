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

// create token to redis
type CreateTokenGroup = {
    userId: string
    clientId: string
    tokens: TokenInfoGroup
}

type TokenPayload = {
    userId: string
    clientId: string
}


export {
    CreateTokenGroup, TokenExpiration, TokenGroup, TokenInfo,
    TokenInfoGroup, TokenPayload
}

