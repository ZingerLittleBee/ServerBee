import { Controller } from '@nestjs/common'
import { AppService } from './app.service'
import { MessagePattern } from '@nestjs/microservices'
import { kTokenVerify, ResultUtil } from '@server-octopus/shared'
import { Result } from '@server-octopus/types'

@Controller()
export class AppController {
    constructor(private readonly appService: AppService) {}

    @MessagePattern(kTokenVerify)
    async isTokenValid(data: { token: string }): Promise<Result<boolean>> {
        try {
            const res = await this.appService.isTokenValid(data.token)
            if (res) {
                return ResultUtil.ok(res)
            }
        } catch (e) {
            return ResultUtil.error(e.message)
        }

        return ResultUtil.error('token invalid')
    }
}
