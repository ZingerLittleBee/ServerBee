import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Reflector } from '@nestjs/core'
import { Request } from 'express'
import { ConfigService } from '@nestjs/config'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'
import { RedisService } from '@app/shared/db/redis.service'
import { JwtUtilService } from '@app/shared/utils/jwt.util.service'

@Injectable()
export class ClientRegisterGuard implements CanActivate {
    private logger = new Logger(ClientRegisterGuard.name)

    constructor(
        private jwtService: JwtService,
        private jwtUtilService: JwtUtilService
    ) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) return true
        try {
            const payload = await this.jwtService.verifyAsync<{
                clientId: string
                iat: number
                exp: number
            }>(token, {
                secret: this.jwtUtilService.getClientAccessSecret()
            })
            request['clientId'] = payload.clientId
        } catch (e) {
            return true
        }
        return true
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        const [type, token] = request.headers.authorization?.split(' ') ?? []
        return type === 'Bearer' ? token : undefined
    }
}
