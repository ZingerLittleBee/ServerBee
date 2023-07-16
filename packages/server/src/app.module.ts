import { Module } from '@nestjs/common'
import { UserModule } from './user/user.module'
import { AuthModule } from './auth/auth.module'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { ProfileModule } from './profile/profile.module'
import { TaskModule } from './task/task.module'
import { ScheduleModule } from '@nestjs/schedule'
import { ClientsModule, Transport } from '@nestjs/microservices'
import {
    kAuthService,
    kNatsServer,
    kStorageService
} from '@server-octopus/shared'

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
                    imports: [ConfigModule],
                    inject: [ConfigService],
                    useFactory: (configService: ConfigService) => ({
                        transport: Transport.NATS,
                        options: {
                            servers: configService.get<string>(kNatsServer)
                        }
                    })
                },
                {
                    name: kStorageService,
                    imports: [ConfigModule],
                    inject: [ConfigService],
                    useFactory: (configService: ConfigService) => ({
                        transport: Transport.NATS,
                        options: {
                            servers: configService.get<string>(kNatsServer)
                        }
                    })
                }
            ]
        }),
        ScheduleModule.forRoot(),
        UserModule,
        AuthModule,
        ProfileModule,
        TaskModule
    ]
})
export class AppModule {}
