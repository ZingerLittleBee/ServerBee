import { Module } from '@nestjs/common'
import { UdService } from './ud.service'
import { UdController } from './ud.controller'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule],
    controllers: [UdController],
    providers: [UdService]
})
export class UdModule {}
