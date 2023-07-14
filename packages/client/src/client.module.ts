import { Module } from '@nestjs/common'

import {
    kAuthService,
    kNatsServer,
    kStorageService
} from '@server-octopus/shared'
import { ClientController } from '@/client.controller'
import { ClientService } from '@/client.service'
import { EventsGateway } from '@/gateway/events.gateway'
import { EventsService } from '@/gateway/events.service'
import { ClientsModule, Transport } from '@nestjs/microservices'
import { ConfigModule, ConfigService } from '@nestjs/config'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        }),
        ClientsModule.registerAsync({
            isGlobal: true,
            clients: [
                {
                    name: kAuthService,
                    inject: [ConfigService],
                    useFactory: (configService: ConfigService) => ({
                        transport: Transport.NATS,
                        options: {
                            servers: [configService.get<string>(kNatsServer)]
                        }
                    })
                },
                {
                    name: kStorageService,
                    inject: [ConfigService],
                    useFactory: (configService: ConfigService) => ({
                        transport: Transport.NATS,
                        options: {
                            servers: [configService.get<string>(kNatsServer)]
                        }
                    })
                }
            ]
        })
    ],
    controllers: [ClientController],
    providers: [ClientService, EventsGateway, EventsService]
})
export class ClientModule {}
