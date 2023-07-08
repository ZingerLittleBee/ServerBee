import { Module } from '@nestjs/common'

import { UserController } from './user.controller'
import { UserService } from 'src/user/user.service'
import { SharedModule } from '@server-octopus/shared'

@Module({
    imports: [SharedModule],
    controllers: [UserController],
    providers: [UserService],
    exports: [UserService]
})
export class UserModule {}
