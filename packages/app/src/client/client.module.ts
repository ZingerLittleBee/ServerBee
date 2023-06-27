import { Module } from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientController } from './client.controller'
import { DbModule } from '@/db/db.module'
import { InfluxModule } from '@/influx/influx.module'
import { JwtModule } from '@nestjs/jwt'
import { UtilModule } from '@/utils/util.module'
import { MongooseModule } from '@nestjs/mongoose'
import { Fusion, FusionSchema } from '@/client/schemas/fusion.schema'
import { EventsGateway } from '@/client/gateway/events.gateway'

@Module({
    imports: [
        MongooseModule.forFeature([
            { name: Fusion.name, schema: FusionSchema }
        ]),
        DbModule,
        InfluxModule,
        JwtModule.register({}),
        UtilModule
    ],
    controllers: [ClientController],
    providers: [ClientService, EventsGateway]
})
export class ClientModule {}
