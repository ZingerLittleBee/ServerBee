import { Module } from '@nestjs/common'
import { ProfileController } from './profile.controller'
import { ProfileService } from '@/profile/profile.service'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule],
    providers: [ProfileService],
    controllers: [ProfileController]
})
export class ProfileModule {}
