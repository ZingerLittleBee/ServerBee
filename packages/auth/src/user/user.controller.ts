import { Controller } from '@nestjs/common'
import { UserService } from './user.service'
import { MessagePattern } from '@nestjs/microservices'
import { kUserRegister, ResultUtil } from '@server-octopus/shared'
import { UserRegisterDto, UserRegisterResult } from '@server-octopus/types'

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
}
