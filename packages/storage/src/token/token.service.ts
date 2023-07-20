import { RedisService } from '@/db/redis.service'
import { Injectable } from '@nestjs/common'

@Injectable()
export class TokenService {
    constructor(private readonly redisService: RedisService) {}
}
