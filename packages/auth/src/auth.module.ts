import { Module } from '@nestjs/common'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { TokenModule } from '@/token/token.module'
import { UserModule } from './user/user.module'
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
                    useFactory: (configService: ConfigService) => ({
                        transport: Transport.NATS,
                        options: {
                            servers: [configService.get<string>(kNatsServer)]
                        }
                    })
                }
            ]
        }),
        TokenModule,
        UserModule
    ]
})
export class AuthModule {}
