import { Module } from '@nestjs/common'
import { DbModule } from '@app/shared/db/db.module'
import { DeviceService } from '@app/client/device/device.service'

@Module({
    imports: [DbModule],
    providers: [DeviceService],
    exports: [DeviceService]
})
export class DeviceModule {}
