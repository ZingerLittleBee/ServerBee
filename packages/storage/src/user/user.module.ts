import { Module } from '@nestjs/common'
import { UserService } from './user.service'
import { UserController } from './user.controller'
import { DbModule } from '@/db/db.module'
import { UtilModule } from '@/utils/util.module'

@Module({
    imports: [DbModule, UtilModule],
    controllers: [UserController],
    providers: [UserService]
})
export class UserModule {}
