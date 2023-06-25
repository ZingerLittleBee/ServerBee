import { Module } from '@nestjs/common';
import { DeviceService } from '@/client/device/device.service';
import { DbModule } from '@/db/db.module';

@Module({
  imports: [DbModule],
  providers: [DeviceService],
  exports: [DeviceService],
})
export class DeviceModule {}
