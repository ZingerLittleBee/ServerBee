import { Controller } from '@nestjs/common'
import { UserService } from './user.service'
import {
    kUserCreateEvent,
    kUserFind,
    kUserVerify,
    ResultUtil
} from '@server-octopus/shared'
import {
    CreateUser,
    FindUserDto,
    Result,
    UserVo,
    VertifyUserDto,
    VertifyUserResult
} from '@server-octopus/types'
import { MessagePattern } from '@nestjs/microservices'

@Controller()
export class UserController {
    constructor(private readonly userService: UserService) {}

    @MessagePattern(kUserCreateEvent)
    async createUser(data: CreateUser): Promise<Result<UserVo>> {
        try {
            return ResultUtil.ok(await this.userService.createUser(data))
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserFind)
    async findUser(data: FindUserDto): Promise<Result<UserVo>> {
        try {
            return ResultUtil.ok(await this.userService.findUser(data))
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserVerify)
    async verifyUser(data: VertifyUserDto): Promise<VertifyUserResult> {
        try {
            return ResultUtil.ok(await this.userService.verifyUser(data))
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
