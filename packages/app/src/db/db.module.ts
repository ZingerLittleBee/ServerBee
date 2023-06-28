import { Module } from '@nestjs/common'
import { PrismaService } from './prisma.service'
import { RedisService } from '@/db/redis.service'
import { MongooseModule } from '@nestjs/mongoose'
import { Fusion, FusionSchema } from '@/db/schemas/fusion.schema'
import { MongoService } from '@/db/mongo.service'

@Module({
    imports: [
        MongooseModule.forFeature([{ name: Fusion.name, schema: FusionSchema }])
    ],
    providers: [PrismaService, RedisService, MongoService],
    exports: [PrismaService, RedisService, MongoService]
})
export class DbModule {}
