import { Injectable } from '@nestjs/common'
import { RedisService } from '@/db/redis.service'
import { CreateTokenGroup } from '@server-octopus/types'

@Injectable()
export class TokenService {
    constructor(private readonly redisService: RedisService) {}

    async createTokenGroup(data: CreateTokenGroup) {
        return this.redisService.createTokenGroup(data)
    }
}
