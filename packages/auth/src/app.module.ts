import { Module } from '@nestjs/common'
import { AppController } from './app.controller'
import { AppService } from './app.service'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { ClientsModule, Transport } from '@nestjs/microservices'
import { kNatsServer, kStorageService } from '@server-octopus/shared'
import { JwtModule } from '@nestjs/jwt'
import { TokenService } from '@/token/token.service'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        }),
        JwtModule.register({}),
        ClientsModule.registerAsync([
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
        ])
    ],

    controllers: [AppController],
    providers: [AppService, TokenService]
})
export class AppModule {}
