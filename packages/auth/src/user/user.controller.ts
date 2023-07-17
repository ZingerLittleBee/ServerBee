import { Controller } from '@nestjs/common'
import { UserService } from './user.service'
import { MessagePattern } from '@nestjs/microservices'
import {
    kUserRegister,
    kUserTokenSign,
    kUserTokenVerify,
    ResultUtil
} from '@server-octopus/shared'
import {
    UserPayload,
    UserRegisterDto,
    UserRegisterResult,
    UserTokenSignResult,
    UserVerifyParam,
    UserVerifyResult
} from '@server-octopus/types'

@Controller()
export class UserController {
    constructor(private readonly userService: UserService) {}

    @MessagePattern(kUserRegister)
    async register(data: UserRegisterDto): Promise<UserRegisterResult> {
        try {
            return ResultUtil.ok(await this.userService.register(data))
        } catch (e) {
            return ResultUtil.error('Register Failed')
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
}
