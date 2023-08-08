import {
    CanActivate,
    ExecutionContext,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ClientService } from '@/client.service'
import { extractAccessToken } from '@server-octopus/shared'

@Injectable()
export class VerifyTokenGuard implements CanActivate {
    private logger = new Logger(VerifyTokenGuard.name)

    constructor(private readonly clientService: ClientService) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = extractAccessToken(request)
        if (!token) {
            this.logger.warn('No token')
            throw new UnauthorizedException('No token')
        }
        const res = await this.clientService.isTokenValid(token)
        if (!res) {
            this.logger.warn(`Token: ${token} verify failed`)
            throw new UnauthorizedException('Token verify failed')
        }
        return true
    }
}
