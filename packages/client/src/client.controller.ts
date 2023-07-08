import {
    Body,
    Controller,
    Logger,
    Post,
    Request,
    UseGuards
} from '@nestjs/common'
import { ClientService } from './client.service'
import { CreateClientDto } from './dto/create-client.dto'
import { ClientRegisterGuard } from '@/guard/register.guard'
import { CreateDeviceDto } from '@/device/dto/create-device.dto'
import { CreateFusionDto } from '@/dto/create-fusion.dto'
import { ClientDataGuard } from '@/guard/data.guard'
import { Result } from '@server-octopus/types'
import { ResultUtil } from '@server-octopus/shared'

@Controller('client')
export class ClientController {
    private readonly logger = new Logger(ClientController.name)

    constructor(private readonly clientService: ClientService) {}

    @UseGuards(ClientRegisterGuard)
    @Post('register')
    async register(
        @Body() device: CreateDeviceDto,
        @Request() req: Request & { clientId?: string; userId?: string }
    ): Promise<Result<{ token?: string }>> {
        try {
            const token = await this.clientService.registerClient(
                new CreateClientDto({
                    name: device?.name,
                    device,
                    userId: req.userId,
                    clientId: req.clientId
                })
            )
            return ResultUtil.ok({
                token
            })
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @UseGuards(ClientDataGuard)
    @Post('data')
    async data(@Body() fusion: CreateFusionDto) {
        try {
            await this.clientService.addData(fusion)
        } catch (e) {
            this.logger.error(`add data: ${fusion}, error: ${e.message}`)
            return ResultUtil.error(e.message)
        }
    }
}
