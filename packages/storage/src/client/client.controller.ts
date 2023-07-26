import { ErrorUtil } from '@/db/error.util'
import { ResultUtil } from '@/utils/result.util'
import { Controller } from '@nestjs/common'
import { EventPattern, MessagePattern } from '@nestjs/microservices'
import {
    kClientCreateMsg,
    kClientDeviceUpdateEvent
} from '@server-octopus/shared'
import {
    ClientStatus,
    CreateClientDto,
    CreateClientResult,
    UpdateDeviceDto
} from '@server-octopus/types'
import { ClientService } from './client.service'

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

    @MessagePattern(kClientCreateMsg)
    async createClient(client: CreateClientDto): Promise<CreateClientResult> {
        try {
            const { client_id, status, name, updated_at } =
                await this.clientService.create(client)
            return ResultUtil.ok({
                clientId: client_id,
                name: name,
                status: status as ClientStatus,
                lastCommunication: updated_at
            })
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }

    @EventPattern(kClientDeviceUpdateEvent)
    async updateClientDevice(data: UpdateDeviceDto) {
        return this.clientService.updateClientDevice(data)
    }
}
