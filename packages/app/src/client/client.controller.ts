import {
    Controller,
    Get,
    Post,
    Body,
    Patch,
    Param,
    Delete,
    RawBodyRequest,
    Req, UseGuards
} from '@nestjs/common'
import { ClientService } from './client.service'
import { CreateClientDto } from './dto/create-client.dto'
import { Result, ResultUtil } from '@/utils/ResultUtil'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'
import { AuthGuard } from '@/auth/auth.guard'

@Controller('client')
export class ClientController {
    constructor(private readonly clientService: ClientService) {}

    @Post('register')
    async register(
        @Body() createClientDto: CreateClientDto
    ): Promise<Result<{ token?: string }>> {
        try {
            const token = await this.clientService.registerClient(
                createClientDto
            )
            return ResultUtil.ok({
                token
            })
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    // add data to influxdb
    // @Post('data')
    // async data(@Req() req: RawBodyRequest<Request>) {
    //     console.log(`rawBody: ${req.rawBody}`)
    //     const data = req.rawBody?.toString('utf-8').split('\n')
    //     data?.forEach((item, index) => {
    //         console.log(`item${index}: ${item}`)
    //     })
    //     return ResultUtil.ok()
    // }

    @UseGuards(AuthGuard)
    @Post('data')
    async data(@Body() fusion: CreateFusionDto) {
        try {
            await this.clientService.addData(fusion)
            return ResultUtil.ok()
        } catch (e) {
            ResultUtil.error(e.message)
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
