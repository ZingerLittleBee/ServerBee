import { Result } from '../result.type'
import { TokenGroup } from '../token/token.type'

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
}

type UserTokenExpiration = {
    accessExpiration: number
    refreshExpiration: number
}

type UserToken = TokenGroup

type UserTokenSignResult = Result<UserToken>

type UserTokenExpirationResult = Result<UserTokenExpiration>

type UserLoginDto = {
    username?: string
    email?: string
    password: string
}

export {
    UserLoginDto,
    UserToken, CreateUser, FindUserDto, UserPayload, UserRefreshPayload, UserRegisterDto, UserRegisterResult, UserTokenExpiration, UserTokenExpirationResult, UserVo, UserTokenSignResult }

