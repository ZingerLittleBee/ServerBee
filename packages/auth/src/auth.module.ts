import { Module } from '@nestjs/common'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { TokenModule } from '@/token/token.module'
import { UserModule } from './user/user.module'
import { ClientsModule, Transport } from '@nestjs/microservices'
import {
    defaultStorageServiceHost,
    defaultStorageServicePort,
    kStorageService,
    kStorageServiceHost,
    kStorageServicePort
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
                        transport: Transport.TCP,
                        options: {
                            host:
                                configService.get<string>(
                                    kStorageServiceHost
                                ) ?? defaultStorageServiceHost,
                            port:
                                configService.get<number>(
                                    kStorageServicePort
                                ) ?? defaultStorageServicePort
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
