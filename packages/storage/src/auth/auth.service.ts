import { Injectable } from '@nestjs/common'
import { RedisService } from '@/db/redis.service'
import { JwtUtilService } from '@/utils/jwt.util.service'

@Injectable()
export class AuthService {
    constructor(
        private readonly redisService: RedisService,
        private readonly jwtUtilService: JwtUtilService
    ) {}

    async verifyTokenInRedis(token: string) {
        this.redisService
    }
}
