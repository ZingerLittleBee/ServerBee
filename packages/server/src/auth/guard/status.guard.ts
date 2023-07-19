import {
    CanActivate,
    ExecutionContext,
    Injectable,
    UnauthorizedException
} from '@nestjs/common'
import { extractAccessTokenFromHeader } from '../auth.util'
import { kAccessToken } from '../auth.const'
import { AuthService } from '../auth.service'

@Injectable()
export class StatusGuard implements CanActivate {
    constructor(private readonly authService: AuthService) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const refreshToken = extractAccessTokenFromHeader(request)
        if (!refreshToken) {
            throw new UnauthorizedException('Access Token Not Found')
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
