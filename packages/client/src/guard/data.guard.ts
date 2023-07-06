import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Request } from 'express'
import { JwtUtilService, RedisService } from '@server-octopus/shared'

@Injectable()
export class ClientDataGuard implements CanActivate {
    private logger = new Logger(ClientDataGuard.name)

    constructor(
        private jwtService: JwtService,
        private redisService: RedisService,
        private jwtUtilService: JwtUtilService
    ) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) {
            this.logger.error(`token not found`)
            throw new UnauthorizedException(`token not found`)
        }
        try {
            const payload = await this.jwtService.verifyAsync<{
                clientId: string
                iat: number
                exp: number
            }>(token, {
                secret: this.jwtUtilService.getClientAccessSecret()
            })
            if (!payload.clientId) {
                this.logger.error(`token: ${token} clientId not found`)
                return false
            }
            JwtUtilService.expireChecker(payload.exp)
            await this.checkerInRedis(token, payload.clientId)
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

    private async checkerInRedis(token: string, clientId: string) {
        try {
            await this.redisService.equal(clientId, token)
        } catch (e) {
            this.logger.error(
                `token: ${token} check in redis failed: not found or not equal`
            )
            throw new UnauthorizedException(`token expired`)
        }
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        const [type, token] = request.headers.authorization?.split(' ') ?? []
        return type === 'Bearer' ? token : undefined
    }
}
