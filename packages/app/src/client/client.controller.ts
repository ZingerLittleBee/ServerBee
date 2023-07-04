import {
    Controller,
    Get,
    Post,
    Body,
    Param,
    Delete,
    UseGuards,
    Logger,
    Request
} from '@nestjs/common'
import { ClientService } from './client.service'
import { CreateClientDto } from './dto/create-client.dto'
import { Result, ResultUtil } from '@/utils/ResultUtil'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'
import { ClientDataGuard } from '@/client/guard/data.guard'
import { ClientRegisterGuard } from '@/client/guard/register.guard'
import { request } from 'express'
import { CreateDeviceDto } from '@/client/device/dto/create-device.dto'

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
            const token = await this.clientService.registerClient({
                name: device?.name,
                device,
                userId: req.userId,
                clientId: req.clientId
            })
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

    @Get()
    findAll() {
        return this.clientService.findAll()
    }

    @Get(':id')
    findOne(@Param('id') id: string) {
        return this.clientService.findOne(+id)
    }

    @Delete(':id')
    remove(@Param('id') id: string) {
        return this.clientService.remove(+id)
    }
}
