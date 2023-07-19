import { Controller } from '@nestjs/common'
import { TokenService } from './token.service'
import { EventPattern } from '@nestjs/microservices'
import { CreateTokenGroup } from '@server-octopus/types'
import { kCreateTokenGroup } from '@server-octopus/shared'

@Controller()
export class TokenController {
    constructor(private readonly tokenService: TokenService) {}

    @EventPattern(kCreateTokenGroup)
    async createTokenGroup(data: CreateTokenGroup) {
        return this.tokenService.createTokenGroup(data)
    }
}
