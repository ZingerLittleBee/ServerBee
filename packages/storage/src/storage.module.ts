import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'
import { DbModule } from '@/db/db.module'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { StorageController } from '@/storage.controller'
import { ProfileModule } from '@/profile/profile.module'
import { UserModule } from './user/user.module'
import { ClientModule } from './client/client.module'
import { MongooseModule } from '@nestjs/mongoose'
import { RedisModule, RedisModuleOptions } from '@liaoliaots/nestjs-redis'
import { kMongoUrl, kRedisUrl } from '@server-octopus/shared'
import { TokenModule } from './token/token.module'
import { UdModule } from './ud/ud.module'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        }),
        MongooseModule.forRootAsync({
            imports: [ConfigModule],
            inject: [ConfigService],
            useFactory: (config: ConfigService) => ({
                uri: config.get<string>(kMongoUrl)
            })
        }),
        RedisModule.forRootAsync({
            imports: [ConfigModule],
            inject: [ConfigService],
            useFactory: (config: ConfigService) => {
                return {
                    readyLog: true,
                    config: {
                        url: config.get(kRedisUrl)
                    }
                } as RedisModuleOptions
            }
        }),
        DbModule,
        UtilModule,
        ProfileModule,
        UserModule,
        ClientModule,
        TokenModule,
        UdModule
    ],
    controllers: [StorageController]
})
export class StorageModule {}
