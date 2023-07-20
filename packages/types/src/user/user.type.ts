import { Result } from '../result.type'
import { TokenExpiration, TokenGroup, TokenPayload } from '../token/token.type'

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

type UserPayload = TokenPayload

type UserRefreshPayload = {
    email?: string
    username?: string
}

type UserTokenExpiration = TokenExpiration

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
type UserTokenRefreshResult = Result<string>

type UserTokenValidParam = { token: string }
type UserTokenValidResult = Result

type VerifyUserResult = Result<{ userId: string }>
type VerifyUserParam = {
    username?: string
    email?: string
    password: string
}

export {
    CreateUser, FindUserDto, UserLoginDto, UserPayload, UserRefreshPayload, UserRegisterDto, UserRegisterResult, UserToken, UserTokenExpiration, UserTokenExpirationResult, UserTokenRefreshParam, UserTokenRefreshResult, UserTokenSignResult, UserTokenValidParam, UserTokenValidResult, UserVerifyParam,
    UserVerifyResult, UserVo, VerifyUserParam, VerifyUserResult
}

