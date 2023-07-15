import { Module } from '@nestjs/common'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { ClientsModule, Transport } from '@nestjs/microservices'
import { kNatsServer, kStorageService } from '@server-octopus/shared'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        }),
        ClientsModule.registerAsync({
            isGlobal: true,
            clients: [
                {
                    name: kStorageService,
                    inject: [ConfigService],
                    useFactory: (configService: ConfigService) => {
                        console.log(
                            'auth module',
                            configService.get<string>(kNatsServer)
                        )
                        return {
                            transport: Transport.NATS,
                            options: {
                                servers: [
                                    configService.get<string>(kNatsServer)
                                ]
                            }
                        }
                    }
                }
            ]
        })
    ]
})
export class AuthModule {}
