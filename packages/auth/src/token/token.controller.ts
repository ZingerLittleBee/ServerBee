import { Controller } from '@nestjs/common'
import { MessagePattern } from '@nestjs/microservices'
import {
    kClientTokenSign,
    kTokenValid,
    kTokenVerify,
    kUserTokenExpirationGet,
    kUserTokenSign,
    ResultUtil
} from '@server-octopus/shared'
import {
    ClientPayload,
    Result,
    UserPayload,
    UserTokenExpirationResult
} from '@server-octopus/types'
import { SignType, TokenService } from '@/token/token.service'

@Controller()
export class TokenController {
    constructor(private readonly tokenService: TokenService) {}

    @MessagePattern(kTokenValid)
    async isTokenValid(data: { token: string }): Promise<Result<boolean>> {
        try {
            const res = await this.tokenService.isTokenValid(data.token)
            return res ? ResultUtil.ok(res) : ResultUtil.error('Token Invalid')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kClientTokenSign)
    async signTokenForClient(payload: ClientPayload) {
        try {
            const token = await this.tokenService.sign(payload, SignType.client)
            return token
                ? ResultUtil.ok(token)
                : ResultUtil.error('Client Sign Error')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenSign)
    async signTokenForUser(payload: UserPayload) {
        try {
            const token = await this.tokenService.sign(payload, SignType.user)
            return token
                ? ResultUtil.ok(token)
                : ResultUtil.error('User Sign Error')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kTokenVerify)
    async verifyToken(data: { token: string }): Promise<Result<any>> {
        try {
            return ResultUtil.ok(await this.tokenService.verify(data.token))
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @MessagePattern(kUserTokenExpirationGet)
    async getUserTokenExpiration(): Promise<UserTokenExpirationResult> {
        try {
            return ResultUtil.ok(this.tokenService.getUserExpiration())
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
