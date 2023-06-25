import {Body, Controller, Post} from '@nestjs/common';
import { InfluxService } from './influx.service';

@Controller('influx')
export class InfluxController {
  constructor(private readonly influxService: InfluxService) {}
}
