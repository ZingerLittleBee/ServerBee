import { Module } from '@nestjs/common'
import { PrismaService } from './prisma.service'
import { MongooseModule } from '@nestjs/mongoose'
import { FusionModel, FusionSchema } from '@/db/schemas/fusion.schema'
import { RedisService } from '@/db/redis.service'
import { MongoService } from '@/db/mongo.service'
import { ErrorUtil } from '@/db/error.util'

@Module({
    imports: [
        MongooseModule.forFeature([
            { name: FusionModel.name, schema: FusionSchema }
        ])
    ],
    providers: [PrismaService, RedisService, MongoService, ErrorUtil],
    exports: [
        PrismaService,
        RedisService,
        MongoService,
        ErrorUtil,
        MongooseModule
    ]
})
export class DbModule {}
