import { PartialType } from '@nestjs/mapped-types'
import { CreateUserDto } from '@app/server/user/dto/create-user.dto'

export class LoginUserDto extends PartialType(CreateUserDto) {}
