import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'
import { kUserTokenExpirationGet, ResultUtil } from '@server-octopus/shared'
import { UserTokenExpirationResult } from '@server-octopus/types'
import { TokenService } from '@/token/token.service'

@Controller()
export class TokenController {
    constructor(private readonly tokenService: TokenService) {}

    @MessagePattern(kUserTokenExpirationGet)
    async getUserTokenExpiration(): Promise<UserTokenExpirationResult> {
        try {
            return ResultUtil.ok(this.tokenService.getUserExpiration())
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
