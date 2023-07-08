import { Controller } from '@nestjs/common'
import { EventPattern, MessagePattern } from '@nestjs/microservices'
import { RedisService } from '@/db/redis.service'
import { EventJwtCreated, FusionDto, Result } from '@server-octopus/types'
import { ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'
import { MongoService } from '@/db/mongo.service'
import {
    kFusionAddEvent,
    kJwtCreatedEvent,
    kRedisEqualEvent
} from '@server-octopus/shared'

@Controller()
export class StorageController {
    constructor(
        private readonly redisService: RedisService,
        private readonly mongoService: MongoService,
        private readonly errorUtil: ErrorUtil
    ) {}

    @EventPattern(kJwtCreatedEvent)
    async handleJwtCreated(data: EventJwtCreated) {
        this.redisService.setWithExpire({ ...data })
    }

    @EventPattern(kFusionAddEvent)
    async addFusion(fusion: FusionDto) {
        this.mongoService.addFusion(fusion)
    }

    @MessagePattern(kRedisEqualEvent)
    async equal(data: { key: string; value: any }): Promise<Result<boolean>> {
        try {
            const res = await this.redisService.equal(data.key, data.value)
            return ResultUtil.ok(res)
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }
}
