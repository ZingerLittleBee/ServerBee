import { Module } from '@nestjs/common'
import { PrismaService } from './prisma.service'
import { MongooseModule } from '@nestjs/mongoose'

import { RedisModule, RedisModuleOptions } from '@liaoliaots/nestjs-redis'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { Fusion, FusionSchema } from '@/db/schemas/fusion.schema'
import { RedisService } from '@/db/redis.service'
import { MongoService } from '@/db/mongo.service'
import { MongoController } from '@/db/mongo.controller'

@Module({
    imports: [
        MongooseModule.forRoot('mongodb://localhost:27017'),
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
        MongooseModule.forFeature([{ name: Fusion.name, schema: FusionSchema }])
    ],
    controllers: [MongoController],
    providers: [PrismaService, RedisService, MongoService],
    exports: [PrismaService, RedisService, MongoService]
})
export class DbModule {}
