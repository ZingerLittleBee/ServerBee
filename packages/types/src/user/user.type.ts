import { Result } from '../result.type'

type CreateUser = {
    username: string
    password: string
    email: string
    salt: number
}

type UserRegisterDto = Omit<CreateUser, 'salt'>

type UserVo = {
    userId: string
    username: string
    email: string
}

type UserRegisterResult = Result<UserVo>

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

export { CreateUser, FindUserDto, UserPayload, UserRefreshPayload, UserRegisterDto, UserRegisterResult, UserTokenExpiration, UserTokenExpirationResult, UserVo }

