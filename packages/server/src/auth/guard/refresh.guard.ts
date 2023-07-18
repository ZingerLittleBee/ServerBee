import {
    CanActivate,
    ExecutionContext,
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import {
    kAuthService,
    kUserTokenRefresh,
    kUserTokenVerify
} from '@server-octopus/shared'
import { firstValueFrom } from 'rxjs'
import {
    UserTokenRefreshParam,
    UserTokenRefreshResult
} from '@server-octopus/types'
import { extractRefreshTokenFromHeader } from '../auth.util'
import { kAccessToken } from '../auth.const'

@Injectable()
export class RefreshGuard implements CanActivate {
    private logger = new Logger(RefreshGuard.name)

    constructor(@Inject(kAuthService) private authClient: ClientProxy) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = extractRefreshTokenFromHeader(request)
        if (!token) {
            throw new UnauthorizedException('Refresh Token Not Found')
        }
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserTokenRefreshResult, UserTokenRefreshParam>(
                kUserTokenRefresh,
                {
                    refreshToken: token
                }
            )
        )
        if (!success || !data) {
            this.logger.error(`Invoke ${kUserTokenVerify} Error: ${message}`)
            throw new UnauthorizedException()
        }
        request[kAccessToken] = data.accessToken
        return true
    }
}
