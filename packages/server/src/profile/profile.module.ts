import { Module } from '@nestjs/common'
import { ProfileService } from './profile.service'
import { ProfileController } from './profile.controller'
import { JwtModule } from '@nestjs/jwt'
import { SharedModule } from '@server-octopus/shared'

@Module({
    imports: [JwtModule.register({}), SharedModule],
    controllers: [ProfileController],
    providers: [ProfileService]
})
export class ProfileModule {}
