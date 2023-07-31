import {
    CanActivate,
    ExecutionContext,
    Injectable,
    UnauthorizedException
} from '@nestjs/common'
import { AuthService } from '../auth.service'
import { extractAccessTokenFromCookie } from '@server-octopus/shared'

@Injectable()
export class StatusGuard implements CanActivate {
    constructor(private readonly authService: AuthService) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const accessToken = extractAccessTokenFromCookie(request)
        if (!accessToken) {
            throw new UnauthorizedException('Access Token Not Found')
        }
        try {
            return await this.authService.checkAccessToken(accessToken)
        } catch (e) {
            throw new UnauthorizedException(e.message)
        }
    }
}
