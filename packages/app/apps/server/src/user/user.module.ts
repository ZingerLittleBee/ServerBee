import { Module } from '@nestjs/common'

import { UserController } from './user.controller'
import { SharedModule } from '@app/shared'
import { UserService } from '@app/server/user/user.service'

@Module({
    imports: [SharedModule],
    controllers: [UserController],
    providers: [UserService],
    exports: [UserService]
})
export class UserModule {}
