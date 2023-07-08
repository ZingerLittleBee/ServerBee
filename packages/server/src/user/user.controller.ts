import { Body, Controller, Post } from '@nestjs/common'
import { UserService } from 'src/user/user.service'
import { CreateUser } from '@server-octopus/types'

@Controller('user')
export class UserController {
    constructor(private readonly userService: UserService) {}

    @Post()
    async createUser(@Body() createUserDto: CreateUser) {
        return this.userService.createUser(createUserDto)
    }
}
