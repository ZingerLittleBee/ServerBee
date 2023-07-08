import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Request } from 'express'
import { JwtUtilService } from '@server-octopus/shared'

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
