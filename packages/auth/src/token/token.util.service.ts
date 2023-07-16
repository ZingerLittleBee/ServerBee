import { ConfigService } from '@nestjs/config'
import { Injectable, NotFoundException } from '@nestjs/common'
import {
    kClientAccessExpiration,
    kClientAccessSecret,
    kUserAccessExpiration,
    kUserAccessSecret,
    kUserRefreshExpiration,
    kUserRefreshSecret
} from '@/token/token.const'

@Injectable()
export class TokenUtilService {
    private defaultClientAccessExpiration = 3600 * 24 * 30
    private defaultUserAccessExpiration = 3600 * 24 * 30
    private defaultUserRefreshExpiration = 3600 * 24 * 30 * 6

    constructor(private readonly configService: ConfigService) {}

    getUserAccessSecret() {
        const userAccessSecret = this.configService.get(kUserAccessSecret)
        if (userAccessSecret) {
            return userAccessSecret
        } else {
            throw new NotFoundException('User Access Secret Not Found')
        }
    }

    getUserRefreshSecret() {
        const userRefreshSecret = this.configService.get(kUserRefreshSecret)
        if (userRefreshSecret) {
            return userRefreshSecret
        } else {
            throw new NotFoundException('User Refresh Secret Not Found')
        }
    }

    getUserAccessExpiration() {
        if (!this.configService.get(kUserAccessExpiration)) {
            return this.defaultUserAccessExpiration
        }
        const expiration = parseInt(
            this.configService.get(kUserAccessExpiration) ?? 'NaN'
        )
        return isNaN(expiration) ? this.defaultUserAccessExpiration : expiration
    }

    getUserRefreshExpiration() {
        if (!this.configService.get(kUserRefreshExpiration)) {
            return this.defaultUserRefreshExpiration
        }
        const expiration = parseInt(
            this.configService.get(kUserRefreshExpiration) ?? 'NaN'
        )
        return isNaN(expiration)
            ? this.defaultUserRefreshExpiration
            : expiration
    }

    getClientAccessSecret() {
        const clientAccessSecret = this.configService.get(kClientAccessSecret)
        if (clientAccessSecret) {
            return clientAccessSecret
        } else {
            throw new NotFoundException('Client Access Secret Not Found')
        }
    }

    getClientAccessExpiration() {
        if (!this.configService.get(kClientAccessExpiration)) {
            return this.defaultClientAccessExpiration
        }
        const expireTime = parseInt(
            this.configService.get(kClientAccessExpiration) ?? 'NaN'
        )
        return isNaN(expireTime)
            ? this.defaultClientAccessExpiration
            : expireTime
    }
}
