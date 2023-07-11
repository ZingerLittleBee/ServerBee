import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { Request } from 'express'
import { ClientService } from '@/client.service'

@Injectable()
export class VerifyTokenGuard implements CanActivate {
    private logger = new Logger(VerifyTokenGuard.name)

    constructor(private readonly clientService: ClientService) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) {
            this.logger.warn('no token')
            throw new UnauthorizedException('no token')
        }
        const res = await this.clientService.verifyToken(token)
        if (!res) {
            this.logger.warn('token verify failed')
            throw new UnauthorizedException('token verify failed')
        }
        return true
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        const [type, token] = request.headers.authorization?.split(' ') ?? []
        return type === 'Bearer' ? token : undefined
    }
}
