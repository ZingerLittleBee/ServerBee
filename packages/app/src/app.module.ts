import { Module } from '@nestjs/common'
import { AppController } from './app.controller'
import { AppService } from './app.service'
import { DbModule } from './db/db.module'
import { UserModule } from './user/user.module'
import { ClientModule } from './client/client.module'
import { AuthModule } from './auth/auth.module'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { ProfileModule } from './profile/profile.module'
import { InfluxModule } from './influx/influx.module'
import { RedisModule, RedisModuleOptions } from '@liaoliaots/nestjs-redis'
import { UtilModule } from '@/utils/util.module'

@Module({
    imports: [
        RedisModule.forRootAsync({
            imports: [ConfigModule],
            inject: [ConfigService],
            useFactory: (config: ConfigService) => {
                return {
                    readyLog: true,
                    config: {
                        host: config.get('REDIS_HOST'),
                        port: config.get('REDIS_PORT')
                    }
                } as RedisModuleOptions
            }
        }),
        DbModule,
        UserModule,
        ClientModule,
        AuthModule,
        UtilModule,
        ConfigModule.forRoot({
            isGlobal: true
        }),
        ProfileModule,
        InfluxModule
    ],
    controllers: [AppController],
    providers: [AppService]
})
export class AppModule {}
