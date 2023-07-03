import { Module } from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientController } from './client.controller'
import { DbModule } from '@/db/db.module'
import { InfluxModule } from '@/influx/influx.module'
import { JwtModule } from '@nestjs/jwt'
import { UtilModule } from '@/utils/util.module'
import { EventsGateway } from '@/client/gateway/events.gateway'
import { EventsService } from '@/client/gateway/events.service'

@Module({
    imports: [DbModule, InfluxModule, JwtModule.register({}), UtilModule],
    controllers: [ClientController],
    providers: [ClientService, EventsGateway, EventsService]
})
export class ClientModule {}
