import { Module } from '@nestjs/common'
import { DeviceService } from '@/device/device.service'
import { DbModule } from '@server-octopus/shared'

@Module({
    imports: [DbModule],
    providers: [DeviceService],
    exports: [DeviceService]
})
export class DeviceModule {}
