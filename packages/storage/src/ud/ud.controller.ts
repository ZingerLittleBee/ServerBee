import { Controller } from '@nestjs/common'
import { UdService } from './ud.service'
import { CreateUdDto, Result } from '@server-octopus/types'
import { MessagePattern } from '@nestjs/microservices'
import { kUserDeviceCreate } from '@server-octopus/shared'
import { ResultUtil } from '@/utils/result.util'

@Controller()
export class UdController {
    constructor(private readonly udService: UdService) {}

    @MessagePattern(kUserDeviceCreate)
    async createUd(data: CreateUdDto): Promise<Result<string>> {
        return ResultUtil.ok(await this.udService.createUd(data))
    }
}
