import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'
import { DbModule } from '@/db/db.module'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { StorageController } from '@/storage.controller'
import { ProfileModule } from '@/profile/profile.module'
import { UserModule } from './user/user.module'
import { ClientModule } from './client/client.module'
import * as process from 'process'
import { MongooseModule } from '@nestjs/mongoose'
import { RedisModule, RedisModuleOptions } from '@liaoliaots/nestjs-redis'
import { kRedisHost, kRedisPort } from '@server-octopus/shared'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true,
            envFilePath: `${process.cwd()}/.env`
        }),
        MongooseModule.forRoot('mongodb://localhost:27017'),
        RedisModule.forRootAsync({
            imports: [ConfigModule],
            inject: [ConfigService],
            useFactory: (config: ConfigService) => {
                return {
                    readyLog: true,
                    config: {
                        host: config.get(kRedisHost),
                        port: config.get(kRedisPort)
                    }
                } as RedisModuleOptions
            }
        }),
        DbModule,
        UtilModule,
        ProfileModule,
        UserModule,
        ClientModule
    ],
    controllers: [StorageController]
})
export class StorageModule {}
