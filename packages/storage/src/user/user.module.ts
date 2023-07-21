import { Module } from '@nestjs/common'
import { UserService } from './user.service'
import { UserController } from './user.controller'
import { DbModule } from '@/db/db.module'
import { UtilModule } from '@/utils/util.module'
import { ProfileModule } from '@/profile/profile.module'

@Module({
    imports: [DbModule, UtilModule, ProfileModule],
    controllers: [UserController],
    providers: [UserService]
})
export class UserModule {}
