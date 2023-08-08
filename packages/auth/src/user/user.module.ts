import { Module } from '@nestjs/common'
import { UserService } from './user.service'
import { UserController } from './user.controller'
import { TokenModule } from '@/token/token.module'

@Module({
    imports: [TokenModule],
    controllers: [UserController],
    providers: [UserService]
})
export class UserModule {}
