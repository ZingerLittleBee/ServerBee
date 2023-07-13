import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { firstValueFrom } from 'rxjs'
import { Result } from '@server-octopus/types'
import { kRedisEqualEvent, kStorageService } from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { TokenUtilService } from '@/token/token.util.service'
import { expireChecker } from '@/token/token.util'

export enum SignType {
    client,
    user
}

@Injectable()
export class TokenService {
    private readonly logger = new Logger(TokenService.name)

    constructor(
        private readonly jwtService: JwtService,
        private readonly tokenUtilService: TokenUtilService,
        @Inject(kStorageService) private client: ClientProxy
    ) {}

    extractClientId(token: string) {
        const payload = this.jwtService.verify(token)
        if (!payload.clientId) {
            throw new UnauthorizedException('invalid token')
        }
        return payload.clientId
    }

    checkExpireTime(token: string) {
        const payload = this.jwtService.verify(token)
        expireChecker(payload.exp)
    }

    async isTokenValid(token: string): Promise<boolean> {
        this.checkExpireTime(token)
        const { success, data, message } = await firstValueFrom(
            this.client.send<Result<boolean>>(kRedisEqualEvent, {
                key: this.extractClientId(token),
                value: token
            })
        )
        if (!success || !data) {
            this.logger.error(`invoke ${kRedisEqualEvent} error: ${message}`)
            return false
        }
    }

    async sign(payload: Record<string, any>, type: SignType) {
        if (type === SignType.client) {
            return this.jwtService.signAsync(payload, {
                secret: this.tokenUtilService.getClientAccessSecret(),
                expiresIn: this.tokenUtilService.getClientAccessExpireTime()
            })
        }
        if (type === SignType.user) {
            return this.jwtService.signAsync(payload, {
                secret: this.tokenUtilService.getUserAccessSecret(),
                expiresIn: this.tokenUtilService.getUserAccessExpireTime()
            })
        }
    }
}
