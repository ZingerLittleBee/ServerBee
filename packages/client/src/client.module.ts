import { Module } from '@nestjs/common'

import {
    kAuthService,
    kAuthServiceHost,
    kAuthServicePort,
    kStorageService,
    kStorageServiceHost,
    kStorageServicePort
} from '@server-octopus/shared'
import { ClientController } from '@/client.controller'
import { ClientService } from '@/client.service'
import { EventsGateway } from '@/gateway/events.gateway'
import { EventsService } from '@/gateway/events.service'
import { ClientProxyFactory, Transport } from '@nestjs/microservices'
import { ConfigModule, ConfigService } from '@nestjs/config'
import {
    defaultAuthServiceHost,
    defaultAuthServicePort,
    defaultStorageServiceHost,
    defaultStorageServicePort
} from '@/const'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        })
    ],
    controllers: [ClientController],
    providers: [
        ClientService,
        EventsGateway,
        EventsService,
        {
            provide: kAuthService,
            inject: [ConfigService],
            useFactory: (configService: ConfigService) =>
                ClientProxyFactory.create({
                    transport: Transport.TCP,
                    options: {
                        host:
                            configService.get<string>(kAuthServiceHost) ??
                            defaultAuthServiceHost,
                        port:
                            configService.get<number>(kAuthServicePort) ??
                            defaultAuthServicePort
                    }
                })
        },
        {
            provide: kStorageService,
            inject: [ConfigService],
            useFactory: (configService: ConfigService) =>
                ClientProxyFactory.create({
                    transport: Transport.TCP,
                    options: {
                        host:
                            configService.get<string>(kStorageServiceHost) ??
                            defaultStorageServiceHost,
                        port:
                            configService.get<number>(kStorageServicePort) ??
                            defaultStorageServicePort
                    }
                })
        }
    ]
})
export class ClientModule {}
