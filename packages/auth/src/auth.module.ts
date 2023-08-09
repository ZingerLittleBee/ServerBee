import { Module } from '@nestjs/common'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { TokenModule } from '@/token/token.module'
import { UserModule } from './user/user.module'
import { ClientsModule, Transport } from '@nestjs/microservices'
import {
    defaultNatsServerUrl,
    kNatsServerUrl,
    kStorageService
} from '@server-octopus/shared'
import { JwtModule } from '@nestjs/jwt'
import { ClientModule } from './client/client.module'

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
                            servers:
                                configService.get<string>(kNatsServerUrl) ??
                                defaultNatsServerUrl
                        }
                    })
                }
            ]
        }),
        JwtModule.register({
            global: true
        }),
        TokenModule,
        UserModule,
        ClientModule
    ]
})
export class AuthModule {}
