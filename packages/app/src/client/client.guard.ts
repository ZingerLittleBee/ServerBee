import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Reflector } from '@nestjs/core'
import { Request } from 'express'
import { ConfigService } from '@nestjs/config'
import { expireChecker } from '@/utils/JwtUtil'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'

@Injectable()
export class ClientAuthGuard implements CanActivate {
    private logger = new Logger(ClientAuthGuard.name)

    constructor(
        private jwtService: JwtService,
        private reflector: Reflector,
        private configService: ConfigService,
        @InjectRedis() private readonly redis: Redis
    ) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) {
            this.logger.error(`token not found`)
            throw new UnauthorizedException()
        }
        try {
            const payload = await this.jwtService.verifyAsync<{
                clientId: string
                iat: number
                exp: number
            }>(token, {
                secret: this.configService.get<string>(
                    'JWT_CLIENT_ACCESS_SECRET'
                )
            })
            expireChecker(payload.exp)
            this.checkerInRedis(token, payload.clientId)
            request['clientId'] = payload.clientId
            this.logger.verbose(
                `token: ${token} verify success, clientId: ${payload.clientId}`
            )
        } catch (e) {
            this.logger.error(`token: ${token} verify failed`)
            throw new UnauthorizedException(e)
        }
        return true
    }

    private checkerInRedis(token: string, clientId: string): void {
        this.redis.get(clientId, (err, res) => {
            if (err) {
                this.logger.error(
                    `token: ${token} check in redis failed: '${err}'`
                )
                throw new UnauthorizedException(err)
            }
            if (!res || res !== token) {
                this.logger.error(
                    `token: ${token} check in redis failed: not found or not equal`
                )
                throw new UnauthorizedException(`token expired`)
            }
        })
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        const [type, token] = request.headers.authorization?.split(' ') ?? []
        return type === 'Bearer' ? token : undefined
    }
}
