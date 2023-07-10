import { Injectable, Logger } from '@nestjs/common'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'

@Injectable()
export class RedisService {
    private logger = new Logger(RedisService.name)

    constructor(@InjectRedis() private readonly redis: Redis) {}

    async setWithExpire(data: { key: string; value: any; expire: number }) {
        return this.redis.set(data.key, data.value, 'EX', data.expire)
    }

    async equal(key: string, value: any) {
        try {
            const res = await this.redis.get(key)
            return res === value
        } catch (e) {
            this.logger.error(`get from redis error: ${e.message}`)
            return false
        }
    }

    async exists(key: string) {
        try {
            const res = await this.redis.exists(key)
            return res === 1
        } catch (e) {
            this.logger.error(`get from redis error: ${e.message}`)
            return false
        }
    }
}
