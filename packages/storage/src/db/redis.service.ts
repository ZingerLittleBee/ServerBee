import { Injectable, Logger } from '@nestjs/common'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'
import { CreateTokenGroup } from '@server-octopus/types'

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

    async createTokenGroup(data: CreateTokenGroup) {
        const { userId, clientId, tokens } = data
        const accessKey = `${userId}:${clientId}:token:access`
        const refreshKey = `${userId}:${clientId}:token:refresh`
        const accessTokenInfo = tokens.access
        const refreshTokenInfo = tokens.refresh
        await this.redis.setex(
            accessKey,
            accessTokenInfo.expiration,
            accessTokenInfo.token
        )
        await this.redis.setex(
            refreshKey,
            refreshTokenInfo.expiration,
            refreshTokenInfo.token
        )
    }
}
