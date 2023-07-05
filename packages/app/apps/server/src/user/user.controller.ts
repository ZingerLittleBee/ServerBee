import { Controller, Post, Body, Get } from '@nestjs/common'
import { UserService } from '@app/server/user/user.service'
import { CreateUserDto } from '@app/server/user/dto/create-user.dto'

@Controller('user')
export class UserController {
    constructor(private readonly userService: UserService) {}

    @Get('all')
    async getAllUsers() {
        return this.userService.getAllUsers()
    }

    @Post()
    async createUser(@Body() createUserDto: CreateUserDto) {
        return this.userService.createUser(createUserDto)
    }
}
