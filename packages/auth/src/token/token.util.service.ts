import { ConfigService } from '@nestjs/config'
import { Injectable, NotFoundException } from '@nestjs/common'
import {
    defaultClientAccessExpiration,
    defaultClientRefreshExpiration,
    defaultSaltRounds,
    defaultUserAccessExpiration,
    defaultUserRefreshExpiration,
    kClientAccessExpiration,
    kClientAccessSecret,
    kClientRefreshExpiration,
    kClientRefreshSecret,
    kSaltRounds,
    kUserAccessExpiration,
    kUserAccessSecret,
    kUserRefreshExpiration,
    kUserRefreshSecret
} from '@/token/token.const'

@Injectable()
export class TokenUtilService {
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
            defaultUserAccessExpiration
        )
    }

    getUserRefreshExpiration() {
        return this.getExpiration(
            kUserRefreshExpiration,
            defaultUserRefreshExpiration
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
            defaultClientRefreshExpiration
        )
    }

    getClientAccessExpiration() {
        return this.getExpiration(
            kClientAccessExpiration,
            defaultClientAccessExpiration
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
            return defaultClientAccessExpiration
        }
        const expiration = parseInt(this.configService.get(key) ?? 'NaN')
        return isNaN(expiration) ? defaultExpiration : expiration
    }

    getSaltRounds() {
        return +(
            this.configService.get<number>(kSaltRounds) ?? defaultSaltRounds
        )
    }
}
