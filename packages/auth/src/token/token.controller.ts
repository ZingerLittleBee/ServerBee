import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'
import {
    kClientTokenSign,
    kTokenVerify,
    kUserTokenSign,
    ResultUtil
} from '@server-octopus/shared'
import { Result } from '@server-octopus/types'
import { SignType, TokenService } from '@/token/token.service'

@Controller()
export class TokenController {
    constructor(private readonly tokenService: TokenService) {}

    @MessagePattern(kTokenVerify)
    async isTokenValid(data: { token: string }): Promise<Result<boolean>> {
        try {
            const res = await this.tokenService.isTokenValid(data.token)
            if (res) {
                return ResultUtil.ok(res)
            }
        } catch (e) {
            return ResultUtil.error(e.message)
        }

        return ResultUtil.error('token invalid')
    }

    @MessagePattern(kClientTokenSign)
    async signTokenForClient(payload: Record<string, any>) {
        try {
            const token = await this.tokenService.sign(payload, SignType.client)
            return ResultUtil.ok(token)
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenSign)
    async signUserForClient(payload: Record<string, any>) {
        try {
            const token = await this.tokenService.sign(payload, SignType.user)
            return ResultUtil.ok(token)
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
