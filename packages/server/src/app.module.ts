import { Module } from '@nestjs/common'
import { UserModule } from './user/user.module'
import { AuthModule } from './auth/auth.module'
import { ConfigModule } from '@nestjs/config'
import { ProfileModule } from './profile/profile.module'
import { TaskModule } from './task/task.module'
import { ScheduleModule } from '@nestjs/schedule'
import { ClientsModule, Transport } from '@nestjs/microservices'
import { kStorageService } from '@server-octopus/shared'

@Module({
    imports: [
        ClientsModule.register({
            clients: [
                {
                    name: kStorageService,
                    transport: Transport.NATS,
                    options: {
                        servers: ['nats://localhost:4222']
                    }
                }
            ],
            isGlobal: true
        }),
        ScheduleModule.forRoot(),
        UserModule,
        AuthModule,
        ConfigModule.forRoot({
            isGlobal: true
        }),
        ProfileModule,
        TaskModule
    ]
})
export class AppModule {}
