import { Module } from '@nestjs/common'
import { ProfileService } from './profile.service'
import { ProfileController } from './profile.controller'
import { JwtModule } from '@nestjs/jwt'
import { SharedModule } from '@app/shared'
import { UserModule } from '@app/server/user/user.module'

@Module({
    imports: [JwtModule.register({}), UserModule, SharedModule],
    controllers: [ProfileController],
    providers: [ProfileService]
})
export class ProfileModule {}
