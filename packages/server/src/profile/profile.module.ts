import { Module } from '@nestjs/common'
import { ProfileService } from './profile.service'
import { ProfileController } from './profile.controller'
import { JwtModule } from '@nestjs/jwt'
import { UserModule } from 'src/user/user.module'
import { SharedModule } from '@server-octopus/shared'

@Module({
    imports: [JwtModule.register({}), UserModule, SharedModule],
    controllers: [ProfileController],
    providers: [ProfileService]
})
export class ProfileModule {}
