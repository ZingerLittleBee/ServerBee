import { Module } from '@nestjs/common'
import { DeviceService } from '@/device/device.service'

@Module({
    providers: [DeviceService],
    exports: [DeviceService]
})
export class DeviceModule {}
