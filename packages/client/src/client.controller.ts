import {
    Body,
    Controller,
    Get,
    Logger,
    Post,
    Request,
    UseGuards
} from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientRegisterGuard } from '@/guard/register.guard'
import { CreateDevice, Result } from '@server-octopus/types'
import { ResultUtil } from '@server-octopus/shared'
import { VerifyTokenGuard } from '@/guard/verify.guard'

@Controller('client')
export class ClientController {
    private readonly logger = new Logger(ClientController.name)

    constructor(private readonly clientService: ClientService) {}

    @UseGuards(ClientRegisterGuard)
    @Post('register')
    async register(
        @Body() device: CreateDevice,
        @Request() req: Request & { clientId?: string; userId?: string }
    ): Promise<Result<{ token: string }>> {
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

    @UseGuards(VerifyTokenGuard)
    @Get('verify')
    async verifyToken() {
        return ResultUtil.ok(true)
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
