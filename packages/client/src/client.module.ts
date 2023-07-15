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
import { ClientProxyFactory, Transport } from '@nestjs/microservices'
import { ConfigModule, ConfigService } from '@nestjs/config'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        })
        // ClientsModule.registerAsync([
        //     {
        //         provide: kAuthService,
        //         imports: [ConfigModule],
        //         inject: [ConfigService],
        //         useFactory: (configService: ConfigService) => {
        //             console.log(
        //                 'configService.get(kNatsServer)',
        //                 configService.get<string>('NATS_SERVER')
        //             )
        //             return ClientProxyFactory.create({
        //                 transport: Transport.NATS,
        //                 options: {
        //                     servers: [configService.get<string>(kNatsServer)]
        //                 }
        //             })
        //         }
        //     }
        //     // {
        //     //     name: kStorageService,
        //     //     imports: [ConfigModule],
        //     //     inject: [ConfigService],
        //     //     useFactory: (configService: ConfigService) => ({
        //     //         transport: Transport.NATS,
        //     //         options: {
        //     //             servers: [configService.get<string>(kNatsServer)]
        //     //         }
        //     //     })
        //     // }
        // ])
    ],
    controllers: [ClientController],
    providers: [
        ClientService,
        EventsGateway,
        EventsService,
        {
            provide: kAuthService,
            inject: [ConfigService],
            useFactory: (configService: ConfigService) => {
                console.log(
                    'configService.get()',
                    configService.get<string>('MONGO_URL')
                )
                return ClientProxyFactory.create({
                    transport: Transport.NATS,
                    options: {
                        servers: [configService.get<string>(kNatsServer)]
                    }
                })
            }
        },
        {
            provide: kStorageService,
            inject: [ConfigService],
            useFactory: (configService: ConfigService) => {
                return ClientProxyFactory.create({
                    transport: Transport.NATS,
                    options: {
                        servers: [configService.get<string>(kNatsServer)]
                    }
                })
            }
        }
    ]
})
export class ClientModule {}
