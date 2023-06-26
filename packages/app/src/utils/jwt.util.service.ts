import { ConfigService } from '@nestjs/config'
import { Injectable, NotFoundException } from '@nestjs/common'
import { kClientAccessExpireTime, kUserAccessSecret } from '@/utils/jwt.const'

@Injectable()
export class JwtUtilService {
    private defaultClientAccessExpireTime = 3600 * 24 * 30

    constructor(private readonly configService: ConfigService) {}

    getUserAccessSecret() {
        const userAccessSecret = this.configService.get(kUserAccessSecret)
        if (userAccessSecret) {
            return userAccessSecret
        } else {
            throw new NotFoundException('UserAccessSecret Not Found')
        }
    }

    getClientAccessExpireTime() {
        if (!this.configService.get(kClientAccessExpireTime)) {
            return this.defaultClientAccessExpireTime
        }
        const expireTime = parseInt(
            this.configService.get(kClientAccessExpireTime)!
        )
        return isNaN(expireTime)
            ? this.defaultClientAccessExpireTime
            : expireTime
    }
}
