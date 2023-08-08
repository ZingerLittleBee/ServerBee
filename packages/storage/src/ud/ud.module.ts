import { Module } from '@nestjs/common'
import { UdService } from './ud.service'
import { UdController } from './ud.controller'
import { DbModule } from '@/db/db.module'
import { UtilModule } from '@/utils/util.module'

@Module({
    imports: [DbModule, UtilModule],
    controllers: [UdController],
    providers: [UdService]
})
export class UdModule {}
