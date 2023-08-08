import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'
import {
    kUserAccessTokenValid,
    kUserRefreshTokenValid,
    kUserRegister,
    kUserTokenRefresh,
    kUserTokenSign,
    kUserTokenVerify,
    ResultUtil
} from '@server-octopus/shared'
import {
    UserPayload,
    UserRegisterDto,
    UserRegisterResult,
    UserTokenRefreshParam,
    UserTokenRefreshResult,
    UserTokenSignResult,
    UserTokenValidParam,
    UserTokenValidResult,
    UserVerifyParam,
    UserVerifyResult
} from '@server-octopus/types'
import { UserService } from './user.service'
import { TokenType } from '@/token/token.const'

@Controller()
export class UserController {
    constructor(private readonly userService: UserService) {}

    @MessagePattern(kUserRegister)
    async register(data: UserRegisterDto): Promise<UserRegisterResult> {
        try {
            return ResultUtil.ok(await this.userService.register(data))
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenSign)
    async sign(payload: UserPayload): Promise<UserTokenSignResult> {
        try {
            const token = await this.userService.sign(payload)
            return token
                ? ResultUtil.ok(token)
                : ResultUtil.error('User Sign Error')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenVerify)
    async verify({ token }: UserVerifyParam): Promise<UserVerifyResult> {
        try {
            const payload = await this.userService.verify(token)
            return payload
                ? ResultUtil.ok(payload)
                : ResultUtil.error('User Token Verify Error')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenRefresh)
    async refreshToken({
        refreshToken
    }: UserTokenRefreshParam): Promise<UserTokenRefreshResult> {
        try {
            return ResultUtil.ok(
                await this.userService.refreshToken(refreshToken)
            )
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserAccessTokenValid)
    async validAccessToken({
        token
    }: UserTokenValidParam): Promise<UserTokenValidResult> {
        try {
            const res = await this.userService.validToken(
                token,
                TokenType.userAccess
            )
            return res ? ResultUtil.ok(res) : ResultUtil.error('Invalid Token')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserRefreshTokenValid)
    async validRefreshToken({
        token
    }: UserTokenValidParam): Promise<UserTokenValidResult> {
        try {
            const res = await this.userService.validToken(
                token,
                TokenType.userRefresh
            )
            return res ? ResultUtil.ok() : ResultUtil.error('Invalid Token')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
