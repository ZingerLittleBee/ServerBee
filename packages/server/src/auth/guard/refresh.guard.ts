import {
    CanActivate,
    ExecutionContext,
    Injectable,
    UnauthorizedException
} from '@nestjs/common'
import { AuthService } from '../auth.service'
import {
    extractRefreshTokenFromCookie,
    kAccessToken
} from '@server-octopus/shared'

@Injectable()
export class RefreshGuard implements CanActivate {
    constructor(private readonly authService: AuthService) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const refreshToken = extractRefreshTokenFromCookie(request)
        if (!refreshToken) {
            throw new UnauthorizedException('Refresh Token Not Found')
        }
        try {
            request[kAccessToken] = await this.authService.refreshToken(
                refreshToken
            )
            return true
        } catch (e) {
            throw new UnauthorizedException(e.message)
        }
    }
}
