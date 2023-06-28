import { Injectable } from '@nestjs/common'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'

@Injectable()
export class RedisService {
    constructor(@InjectRedis() private readonly redis: Redis) {}

    async setWithExpire(key: string, value: any, expire: number) {
        return this.redis.set(key, value, 'EX', expire)
    }
}
