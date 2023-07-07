import { Controller } from '@nestjs/common'
import { EventPattern, MessagePattern } from '@nestjs/microservices'
import { RedisService } from '@/db/redis.service'
import {
    ClientDto,
    DeviceDto,
    DiskDetailDto,
    EventJwtCreated,
    NetworkInfoDto
} from '@server-octopus/types'
import { PrismaService } from '@/db/prisma.service'
import { Result, ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'

@Controller()
export class StorageController {
    constructor(
        private readonly redisService: RedisService,
        private readonly prismaService: PrismaService,
        private readonly errorUtil: ErrorUtil
    ) {}

    @MessagePattern({ cmd: 'sum' })
    accumulate(data: number[]): number {
        return (data || []).reduce((a, b) => a + b)
    }

    @EventPattern('jwt_created')
    async handleJwtCreated(data: EventJwtCreated) {
        this.redisService.setWithExpire({ ...data })
    }

    @MessagePattern('get_client_by_client_id')
    async getClientByClientId(clientId: string): Promise<Result<ClientDto>> {
        try {
            return ResultUtil.ok(
                await this.prismaService.getClientByClientId(clientId)
            )
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }
}
