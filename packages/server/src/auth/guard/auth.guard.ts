import {
    CanActivate,
    ExecutionContext,
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import { kAuthService, kUserTokenVerify } from '@server-octopus/shared'
import { firstValueFrom } from 'rxjs'
import { UserVerifyParam, UserVerifyResult } from '@server-octopus/types'
import { extractAccessTokenFromHeader } from '../auth.util'

@Injectable()
export class AuthGuard implements CanActivate {
    private logger = new Logger(AuthGuard.name)

    constructor(@Inject(kAuthService) private authClient: ClientProxy) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = extractAccessTokenFromHeader(request)
        if (!token) {
            throw new UnauthorizedException()
        }
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserVerifyResult, UserVerifyParam>(
                kUserTokenVerify,
                {
                    token
                }
            )
        )
        if (!success || !data) {
            this.logger.error(`Invoke ${kUserTokenVerify} Error: ${message}`)
            throw new UnauthorizedException()
        }
        request['userId'] = data.userId
        return true
    }
}
