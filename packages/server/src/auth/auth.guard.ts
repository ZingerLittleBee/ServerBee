import {
    CanActivate,
    ExecutionContext,
    Inject,
    Injectable,
    UnauthorizedException
} from '@nestjs/common'
import { Request } from 'express'
import { ClientProxy } from '@nestjs/microservices'
import { kAuthService, kTokenVerify } from '@server-octopus/shared'
import { firstValueFrom } from 'rxjs'
import { Result, UserPayload } from '@server-octopus/types'

@Injectable()
export class AuthGuard implements CanActivate {
    constructor(@Inject(kAuthService) private authClient: ClientProxy) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) {
            throw new UnauthorizedException()
        }
        try {
            const { success, data, message } = await firstValueFrom(
                this.authClient.send<Result<UserPayload>>(kTokenVerify, {
                    token
                })
            )
            if (!success || !data) throw new UnauthorizedException(message)
            request['userId'] = data.userId
        } catch (e) {
            throw new UnauthorizedException(e.message)
        }
        return true
    }

    private extractTokenFromHeader(request: Request): string | undefined {
        return request.cookies['access_token']
    }
}
