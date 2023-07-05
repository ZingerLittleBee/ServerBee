import { Injectable } from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'

@Injectable()
export class EventsService {
    constructor(private readonly jwtService: JwtService) {}

    extractClientIdFromToken(token: string) {
        const payload = this.jwtService.verify<{
            clientId: string
            exp: number
            iat: number
        }>(token)
        return payload.clientId
    }
}
