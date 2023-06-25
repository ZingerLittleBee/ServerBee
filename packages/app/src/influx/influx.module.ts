import { Module } from '@nestjs/common';
import { InfluxService } from './influx.service';
import { InfluxController } from './influx.controller';

@Module({
  controllers: [InfluxController],
  providers: [InfluxService],
  exports: [InfluxService],
})
export class InfluxModule {}
