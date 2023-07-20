import { SignType, TokenType } from '@/token/token.const'
import { expireChecker } from '@/token/token.util'
import { TokenUtilService } from '@/token/token.util.service'
import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { ClientProxy } from '@nestjs/microservices'
import {
    kJwtCreatedEvent,
    kRedisEqualEvent,
    kStorageService
} from '@server-octopus/shared'
import {
    EventJwtCreated,
    Result,
    TokenGroup,
    TokenPayload,
    UserTokenExpiration
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'

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
            this.logger.error(`Invoke ${kRedisEqualEvent} Error: ${message}`)
            return false
        }
    }

    async verify<T extends Record<string, any>>(
        token: string,
        tokenType: TokenType
    ) {
        try {
            return this.jwtService.verifyAsync<T>(token, {
                secret: this.getSecret(tokenType)
            })
        } catch (e) {
            this.logger.error(`Verify Token Error: ${e.message}`)
            throw new UnauthorizedException('invalid token')
        }
    }

    async sign(
        payload: TokenPayload,
        key: string,
        options: {
            secret: string
            expiresIn: number
        }
    ) {
        try {
            const token = await this.jwtService.signAsync(payload, options)
            if (token) {
                this.client.emit<unknown, EventJwtCreated>(kJwtCreatedEvent, {
                    key: key,
                    value: token,
                    expire: options.expiresIn
                })
            }
            return token
        } catch (e) {
            this.logger.error(`Sign Token Error: ${e.message}`)
            throw new Error(`Sign Token Error`)
        }
    }

    async signGroup(
        payload: TokenPayload,
        type: SignType
    ): Promise<TokenGroup> {
        const { accessOptions, refreshOptions } = this.getOptions(type)
        const accessToken = await this.sign(
            payload,
            this.getAccessKey(payload),
            accessOptions
        )
        const refreshToken = await this.sign(
            payload,
            this.getRefreshKey(payload),
            refreshOptions
        )
        return {
            accessToken,
            refreshToken
        }
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

    async tokenValidInStorage(token: string) {}

    getAccessKey(payload: TokenPayload) {
        return `${payload.userId}:${payload.clientId}:token:access`
    }

    getRefreshKey(payload: TokenPayload) {
        return `${payload.userId}:${payload.clientId}:token:refresh`
    }
}
