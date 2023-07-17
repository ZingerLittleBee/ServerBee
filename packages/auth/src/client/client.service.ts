import { Injectable } from '@nestjs/common'
import { TokenService } from '@/token/token.service'
import { ClientPayload } from '@server-octopus/types'
import { SignType, TokenType } from '@/token/token.const'

@Injectable()
export class ClientService {
    constructor(private tokenService: TokenService) {}

    sign(payload: ClientPayload) {
        return this.tokenService.sign(payload, SignType.client)
    }

    verify(token: string) {
        return this.tokenService.verify(token)
    }

    isTokenValid(token: string) {
        return this.tokenService.isTokenValid(token, TokenType.clientAccess)
    }
}
