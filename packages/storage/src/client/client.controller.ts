import { Controller } from '@nestjs/common'
import { ClientService } from './client.service'
import { MessagePattern } from '@nestjs/microservices'
import { kClientUpsertEvent } from '@server-octopus/shared'
import { ClientDto, Result } from '@server-octopus/types'
import { ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'

@Controller()
export class ClientController {
    constructor(
        private readonly clientService: ClientService,
        private readonly errorUtil: ErrorUtil
    ) {}

    @MessagePattern(kClientUpsertEvent)
    async upsertClient(client: ClientDto): Promise<Result<string>> {
        try {
            return ResultUtil.ok(await this.clientService.upsertClient(client))
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }
}
