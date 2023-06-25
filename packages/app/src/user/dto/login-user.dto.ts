import { PartialType } from '@nestjs/mapped-types'
import { CreateUserDto } from '@/user/dto/create-user.dto'

export class LoginUserDto extends PartialType(CreateUserDto) {}
