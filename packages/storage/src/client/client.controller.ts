import { Controller } from '@nestjs/common'
import { ClientService } from './client.service'
import { EventPattern, MessagePattern } from '@nestjs/microservices'
import {
    kClientCreateEvent,
    kClientDeviceUpdateEvent
} from '@server-octopus/shared'
import { CreateClientDto, UpdateDeviceDto } from '@server-octopus/types'
import { ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'

@Controller()
export class ClientController {
    constructor(
        private readonly clientService: ClientService,
        private readonly errorUtil: ErrorUtil
    ) {}

    // @MessagePattern(kClientUpsertEvent)
    // async upsertClient(client: ClientDto): Promise<Result<string>> {
    //     try {
    //         return ResultUtil.ok(await this.clientService.upsertClient(client))
    //     } catch (e) {
    //         return ResultUtil.error(this.errorUtil.explain(e))
    //     }
    // }

    @MessagePattern(kClientCreateEvent)
    async createClient(client: CreateClientDto) {
        try {
            return ResultUtil.ok(await this.clientService.createClient(client))
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }

    @EventPattern(kClientDeviceUpdateEvent)
    async updateClientDevice(data: UpdateDeviceDto) {
        return this.clientService.updateClientDevice(data)
    }
}
