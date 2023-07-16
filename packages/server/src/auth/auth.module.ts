import { Module } from '@nestjs/common'
import { AuthController } from './auth.controller'
import { AuthService } from './auth.service'
import { UserModule } from 'src/user/user.module'
import { kAuthService, kNatsServer } from '@server-octopus/shared'
import { ConfigService } from '@nestjs/config'
import { ClientProxyFactory, Transport } from '@nestjs/microservices'

@Module({
    imports: [UserModule],
    controllers: [AuthController],
    providers: [
        AuthService,
        {
            provide: kAuthService,
            inject: [ConfigService],
            useFactory: (configService: ConfigService) =>
                ClientProxyFactory.create({
                    transport: Transport.NATS,
                    options: {
                        servers: [configService.get<string>(kNatsServer)]
                    }
                })
        }
    ],
    exports: [AuthService]
})
export class AuthModule {}
