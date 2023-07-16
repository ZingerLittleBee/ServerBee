import { Module } from '@nestjs/common'
import { ProfileService } from './profile.service'
import { ProfileController } from './profile.controller'

@Module({
    controllers: [ProfileController],
    providers: [ProfileService]
})
export class ProfileModule {}
