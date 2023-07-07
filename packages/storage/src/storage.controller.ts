import { Controller } from '@nestjs/common'
import { EventPattern, MessagePattern } from '@nestjs/microservices'
import { RedisService } from '@/db/redis.service'
import { ClientDto, EventJwtCreated, FusionDto } from '@server-octopus/types'
import { PrismaService } from '@/db/prisma.service'
import { Result, ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'
import { MongoService } from '@/db/mongo.service'
import {
    kClientUpsertEvent,
    kFusionAddEvent,
    kJwtCreatedEvent
} from '@server-octopus/shared'

@Controller()
export class StorageController {
    constructor(
        private readonly redisService: RedisService,
        private readonly prismaService: PrismaService,
        private readonly mongoService: MongoService,
        private readonly errorUtil: ErrorUtil
    ) {}

    @MessagePattern({ cmd: 'sum' })
    accumulate(data: number[]): number {
        return (data || []).reduce((a, b) => a + b)
    }

    @EventPattern(kJwtCreatedEvent)
    async handleJwtCreated(data: EventJwtCreated) {
        this.redisService.setWithExpire({ ...data })
    }

    @EventPattern(kFusionAddEvent)
    async addFusion(fusion: FusionDto) {
        this.mongoService.addFusion(fusion)
    }

    @MessagePattern(kClientUpsertEvent)
    async upsertClient(client: ClientDto): Promise<Result<string>> {
        try {
            return ResultUtil.ok(await this.prismaService.upsertClient(client))
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }
}
