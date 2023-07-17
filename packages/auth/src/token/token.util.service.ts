import { ConfigService } from '@nestjs/config'
import { Injectable, NotFoundException } from '@nestjs/common'
import {
    kClientAccessExpiration,
    kClientAccessSecret,
    kClientRefreshExpiration,
    kClientRefreshSecret,
    kUserAccessExpiration,
    kUserAccessSecret,
    kUserRefreshExpiration,
    kUserRefreshSecret
} from '@/token/token.const'

@Injectable()
export class TokenUtilService {
    private defaultClientAccessExpiration = 3600 * 24 * 30
    private defaultClientRefreshExpiration = 3600 * 24 * 30 * 6
    private defaultUserAccessExpiration = 3600 * 24 * 30
    private defaultUserRefreshExpiration = 3600 * 24 * 30 * 6

    constructor(private readonly configService: ConfigService) {}

    getUserAccessSecret() {
        return this.getSecret(kUserAccessSecret)
    }

    getUserRefreshSecret() {
        return this.getSecret(kUserRefreshSecret)
    }

    getUserAccessExpiration() {
        return this.getExpiration(
            kUserAccessExpiration,
            this.defaultUserAccessExpiration
        )
    }

    getUserRefreshExpiration() {
        return this.getExpiration(
            kUserRefreshExpiration,
            this.defaultUserRefreshExpiration
        )
    }

    getClientAccessSecret() {
        return this.getSecret(kClientAccessSecret)
    }

    getClientRefreshSecret() {
        return this.getSecret(kClientRefreshSecret)
    }

    getClientRefreshExpiration() {
        return this.getExpiration(
            kClientRefreshExpiration,
            this.defaultClientRefreshExpiration
        )
    }

    getClientAccessExpiration() {
        return this.getExpiration(
            kClientAccessExpiration,
            this.defaultClientAccessExpiration
        )
    }

    getSecret(key: string): string {
        const secret = this.configService.get(key)
        if (secret) {
            return secret
        } else {
            throw new NotFoundException(`${key} Not Found`)
        }
    }

    getExpiration(key: string, defaultExpiration: number) {
        if (!this.configService.get(key)) {
            return this.defaultClientAccessExpiration
        }
        const expiration = parseInt(this.configService.get(key) ?? 'NaN')
        return isNaN(expiration) ? defaultExpiration : expiration
    }
}
