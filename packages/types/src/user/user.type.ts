import { Result } from '../result.type'
import { TokenGroup } from '../token/token.type'

type CreateUser = {
    email: string
    username: string
    password: string
}

type UserRegisterDto = CreateUser

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

type UserVerifyResult = Result<UserPayload>

type UserVerifyParam = {
    token: string
}

type UserTokenRefreshParam = Omit<UserToken, 'accessToken'>
type UserTokenRefreshResult = Result<Omit<UserToken, 'refreshToken'>>

export {
    CreateUser, FindUserDto, UserLoginDto, UserPayload, UserRefreshPayload, UserRegisterDto, UserRegisterResult, UserToken, UserTokenExpiration, UserTokenExpirationResult, UserTokenRefreshParam, UserTokenRefreshResult, UserTokenSignResult, UserVerifyParam,
    UserVerifyResult, UserVo
}

