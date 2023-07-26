import {
    Body,
    Controller,
    Get,
    HttpCode,
    Logger,
    Post,
    Request,
    UseGuards
} from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientRegisterGuard } from '@/guard/register.guard'
import { CreateClientDto, CreateDevice, Result } from '@server-octopus/types'
import { ResultUtil } from '@server-octopus/shared'
import { VerifyTokenGuard } from '@/guard/verify.guard'
import { convertFormatDataToString } from '@/util'

@Controller('client')
export class ClientController {
    private readonly logger = new Logger(ClientController.name)

    constructor(private readonly clientService: ClientService) {}

    @UseGuards(ClientRegisterGuard)
    @Post('register')
    async register(
        @Body() device: CreateDevice,
        @Request() req: Request & { clientId: string; userId: string }
    ): Promise<Result<{ token: string }>> {
        try {
            const token = await this.clientService.registerClient({
                ...convertFormatDataToString(device),
                clientId: req.clientId,
                userId: req.userId
            })
            return ResultUtil.ok({
                token
            })
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @UseGuards(VerifyTokenGuard)
    @Get('verify')
    @HttpCode(200)
    async tokenValidCheck() {
        return ResultUtil.ok()
    }

    @Post()
    async create(client: CreateClientDto) {
        try {
            await this.clientService.create(client)
            return ResultUtil.ok()
        } catch (e) {
            this.logger.error(`create client: ${client}, error: ${e.message}`)
            return ResultUtil.error(e.message)
        }
    }

    // @UseGuards(ClientDataGuard)
    // @Post('data')
    // async data(@Body() fusion: Fusion) {
    //     try {
    //         await this.clientService.addData(fusion)
    //     } catch (e) {
    //         this.logger.error(`add data: ${fusion}, error: ${e.message}`)
    //         return ResultUtil.error(e.message)
    //     }
    // }
}
