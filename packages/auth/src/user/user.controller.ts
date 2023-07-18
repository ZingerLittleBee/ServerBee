import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'
import {
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
    UserVerifyParam,
    UserVerifyResult
} from '@server-octopus/types'
import { UserService } from './user.service'

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
}
