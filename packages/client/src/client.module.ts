import { Module } from '@nestjs/common'

import { JwtModule } from '@nestjs/jwt'
import { SharedModule } from '@server-octopus/shared'
import { ClientController } from '@/client.controller'
import { ClientService } from '@/client.service'
import { EventsGateway } from '@/gateway/events.gateway'
import { EventsService } from '@/gateway/events.service'
import { ClientsModule, Transport } from '@nestjs/microservices'

@Module({
    imports: [
        ClientsModule.register([
            {
                name: 'STORAGE_SERVICE',
                transport: Transport.NATS,
                options: {
                    servers: ['nats://localhost:4222']
                }
            }
        ]),
        SharedModule,
        JwtModule.register({})
    ],
    controllers: [ClientController],
    providers: [ClientService, EventsGateway, EventsService]
})
export class ClientModule {}
