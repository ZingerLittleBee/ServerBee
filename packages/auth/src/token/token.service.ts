import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { firstValueFrom } from 'rxjs'
import {
    ClientPayload,
    EventJwtCreated,
    Result,
    TokenGroup,
    UserPayload,
    UserTokenExpiration
} from '@server-octopus/types'
import {
    kJwtCreatedEvent,
    kRedisEqualEvent,
    kStorageService
} from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { TokenUtilService } from '@/token/token.util.service'
import { expireChecker } from '@/token/token.util'
import { SignType, TokenType } from '@/token/token.const'

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

    checkExpireTime(token: string, secret: string) {
        const payload = this.jwtService.verify(token, { secret })
        expireChecker(payload.exp)
    }

    async isTokenValid(token: string, tokenType: TokenType): Promise<boolean> {
        this.checkExpireTime(token, this.getSecret(tokenType))
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

    async sign(
        payload: ClientPayload | UserPayload,
        type: SignType
    ): Promise<TokenGroup> {
        const { accessOptions, refreshOptions } = this.getOptions(type)
        const accessToken = await this.jwtService.signAsync(
            payload,
            accessOptions
        )
        const refreshToken = await this.jwtService.signAsync(
            payload,
            refreshOptions
        )
        if (accessToken && refreshToken) {
            const isClientPayload = 'clientId' in payload
            this.client.emit<unknown, EventJwtCreated>(kJwtCreatedEvent, {
                key: isClientPayload ? payload.clientId : payload.userId,
                value: accessToken,
                expire: accessOptions.expiresIn
            })
            this.client.emit<unknown, EventJwtCreated>(kJwtCreatedEvent, {
                key: isClientPayload ? payload.clientId : payload.userId,
                value: refreshToken,
                expire: refreshOptions.expiresIn
            })
        }
        return {
            accessToken,
            refreshToken
        }
    }

    async verify(token: string) {
        return this.jwtService.verifyAsync<{
            clientId: string
            userId?: string
            iat: number
            exp: number
        }>(token, {
            secret: this.tokenUtilService.getClientAccessSecret()
        })
    }

    getUserExpiration(): UserTokenExpiration {
        return {
            accessExpiration: this.tokenUtilService.getUserAccessExpiration(),
            refreshExpiration: this.tokenUtilService.getUserRefreshExpiration()
        }
    }

    getSecret(type: TokenType) {
        try {
            if (type === TokenType.clientAccess) {
                return this.tokenUtilService.getClientAccessSecret()
            }
            if (type === TokenType.userAccess) {
                return this.tokenUtilService.getUserAccessSecret()
            }
            if (type === TokenType.userRefresh) {
                return this.tokenUtilService.getUserRefreshSecret()
            }
            if (type === TokenType.clientRefresh) {
                return this.tokenUtilService.getClientRefreshSecret()
            }
        } catch (e) {
            this.logger.error(`Get ${type} Secret Error: ${e.message}`)
            throw new Error(`Invalid Token Type`)
        }
    }

    getAccessSignOptions(type: SignType) {
        return type === SignType.client
            ? {
                  secret: this.tokenUtilService.getClientAccessSecret(),
                  expiresIn: this.tokenUtilService.getClientAccessExpiration()
              }
            : {
                  secret: this.tokenUtilService.getUserAccessSecret(),
                  expiresIn: this.tokenUtilService.getUserAccessExpiration()
              }
    }

    getRefreshSignOptions(type: SignType) {
        return type === SignType.client
            ? {
                  secret: this.tokenUtilService.getClientRefreshSecret(),
                  expiresIn: this.tokenUtilService.getClientRefreshExpiration()
              }
            : {
                  secret: this.tokenUtilService.getUserRefreshSecret(),
                  expiresIn: this.tokenUtilService.getUserRefreshExpiration()
              }
    }

    getOptions(type: SignType) {
        return {
            accessOptions: this.getAccessSignOptions(type),
            refreshOptions: this.getRefreshSignOptions(type)
        }
    }
}
