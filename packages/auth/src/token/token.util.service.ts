import { ConfigService } from '@nestjs/config'
import { Injectable, NotFoundException } from '@nestjs/common'
import {
    kClientAccessExpireTime,
    kClientAccessSecret,
    kUserAccessExpireTime,
    kUserAccessSecret
} from '@/token/token.const'

@Injectable()
export class TokenUtilService {
    private defaultClientAccessExpireTime = 3600 * 24 * 30
    private defaultUserAccessExpireTime = 3600 * 24 * 30

    constructor(private readonly configService: ConfigService) {}

    getUserAccessSecret() {
        const userAccessSecret = this.configService.get(kUserAccessSecret)
        if (userAccessSecret) {
            return userAccessSecret
        } else {
            throw new NotFoundException('user access secret Not Found')
        }
    }

    getUserAccessExpireTime() {
        if (!this.configService.get(kUserAccessExpireTime)) {
            return this.defaultClientAccessExpireTime
        }
        const expireTime = parseInt(
            this.configService.get(kUserAccessExpireTime) ?? 'NaN'
        )
        return isNaN(expireTime) ? this.defaultUserAccessExpireTime : expireTime
    }

    getClientAccessSecret() {
        const clientAccessSecret = this.configService.get(kClientAccessSecret)
        if (clientAccessSecret) {
            return clientAccessSecret
        } else {
            throw new NotFoundException('Client Access Secret Not Found')
        }
    }

    getClientAccessExpireTime() {
        if (!this.configService.get(kClientAccessExpireTime)) {
            return this.defaultClientAccessExpireTime
        }
        const expireTime = parseInt(
            this.configService.get(kClientAccessExpireTime) ?? 'NaN'
        )
        return isNaN(expireTime)
            ? this.defaultClientAccessExpireTime
            : expireTime
    }
}
