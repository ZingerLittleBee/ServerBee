import {
    CanActivate,
    ExecutionContext,
    Inject,
    Injectable,
    Logger
} from '@nestjs/common'
import { Request } from 'express'
import { firstValueFrom } from 'rxjs'
import { kAuthService, kTokenVerify } from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { ClientPayload, Result } from '@server-octopus/types'

@Injectable()
export class ClientRegisterGuard implements CanActivate {
    private logger = new Logger(ClientRegisterGuard.name)

    constructor(@Inject(kAuthService) private authClient: ClientProxy) {}

    async canActivate(context: ExecutionContext): Promise<boolean> {
        const request = context.switchToHttp().getRequest()
        const token = this.extractTokenFromHeader(request)
        if (!token) return true
        try {
            const { success, data } = await firstValueFrom(
                this.authClient.send<Result<ClientPayload>>(kTokenVerify, {
                    token
                })
            )
            if (success && data) {
                request['clientId'] = data.clientId
                request['userId'] = data.userId
            }
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
