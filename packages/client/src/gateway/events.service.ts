import { Inject, Injectable, UnauthorizedException } from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import { kAuthService, kClientTokenVerify } from '@server-octopus/shared'
import { firstValueFrom } from 'rxjs'
import { ClientPayload, Result } from '@server-octopus/types'

@Injectable()
export class EventsService {
    constructor(@Inject(kAuthService) private authService: ClientProxy) {}

    extractBearerTokenFromRawHeaders(rawHeaders: string[]): string {
        const authorizationHeader = rawHeaders.find((header) =>
            header.startsWith('Bearer')
        )
        if (!authorizationHeader) {
            throw new UnauthorizedException('Authorization header not found')
        }
        const token = authorizationHeader.split(' ')[1]
        if (!token) {
            throw new UnauthorizedException('token not found')
        }
        return token
    }

    async extractClientIdFromToken(token: string) {
        const { success, message, data } = await firstValueFrom(
            this.authService.send<Result<ClientPayload>>(kClientTokenVerify, {
                token
            })
        )
        if (!success || !data?.clientId) {
            throw new UnauthorizedException(message)
        }
        return data.clientId
    }
}
