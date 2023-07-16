import { Result } from '../result.type'

type CreateUser = {
    username: string
    password: string
    email: string
}

type UserVo = {
    userId: string
    username: string
    email: string
}

type FindUserDto = {
    username?: string
    email?: string
    userId?: string
}

type UserPayload = {
    userId: string
}

type UserRefreshPayload = {
    email?: string
    username?: string
    password: string
}

type UserTokenExpiration = {
    accessExpiration: number
    refreshExpiration: number
}

type UserTokenExpirationResult = Result<UserTokenExpiration>

export { CreateUser, FindUserDto, UserVo, UserPayload, UserRefreshPayload, UserTokenExpirationResult, UserTokenExpiration }

