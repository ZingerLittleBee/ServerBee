import {
    MessageBody,
    OnGatewayConnection,
    SubscribeMessage,
    WebSocketGateway,
    WebSocketServer,
    WsException
} from '@nestjs/websockets'
import { Injectable, Logger } from '@nestjs/common'
import { ClientService } from '@/client/client.service'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'
import { Server, Socket } from 'socket.io'
import { inspect } from 'util'
import { JwtUtilService } from '@/utils/jwt.util.service'

@Injectable()
@WebSocketGateway(9876, {
    cors: {
        origin: '*'
    }
})
// @UseGuards(WsAuthGuard)
export class EventsGateway implements OnGatewayConnection {
    @WebSocketServer()
    server: Server

    private clients: Map<string, any> = new Map()

    private logger: Logger = new Logger(EventsGateway.name)

    constructor(private readonly clientService: ClientService) {}

    handleConnection(client: Socket, ...args: any[]) {
        // console.log(`handleConnection: ${inspect(client)}`)
        try {
            const token = JwtUtilService.extractBearerTokenFromRawHeaders(
                args[0].rawHeaders
            )
            // parse token
        } catch (e) {
            this.logger.error(e)
            throw new WsException('Invalid credentials.')
        }

        // console.log(`args: ${inspect(args)}`)
        // console.log(`args: ${inspect(args)}`)
        // this.clients.set(client.id, client)
    }

    @SubscribeMessage('report')
    findAll(@MessageBody() fusion: CreateFusionDto) {
        // this.clientService.addData(fusion)
        console.log(`report: ${inspect(fusion)}`)
    }

    @SubscribeMessage('identity')
    async identity(@MessageBody() data: number): Promise<number> {
        console.log(`id: ${data}`)
        return data
    }

    @SubscribeMessage('test')
    async test(@MessageBody() test: any) {
        console.log(`test: ${inspect(test)}`)
        return test
    }
}
