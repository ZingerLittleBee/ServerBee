import {
    ConnectedSocket,
    MessageBody,
    OnGatewayConnection,
    SubscribeMessage,
    WebSocketGateway,
    WebSocketServer,
    WsException
} from '@nestjs/websockets'
import { Injectable, Logger } from '@nestjs/common'
import { inspect } from 'util'
import { Server } from 'ws'
import { JwtUtilService } from '@app/shared/utils/jwt.util.service'
import { EventsService } from '@app/client/gateway/events.service'
import { ClientService } from '@app/client/client.service'
import { CreateFusionDto } from '@app/client/dto/create-fusion.dto'

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

    constructor(
        private readonly clientService: ClientService,
        private readonly eventsService: EventsService
    ) {}

    handleConnection(client: any, ...args: any[]) {
        // console.log(`handleConnection: ${inspect(client)}`)
        try {
            const token = JwtUtilService.extractBearerTokenFromRawHeaders(
                args[0].rawHeaders
            )
            // parse token
            const clientId = this.eventsService.extractClientIdFromToken(token)
            this.logger.log(`client connected: ${clientId}`)
            this.clients.set(clientId, client)
        } catch (e) {
            this.logger.error(e)
            throw new WsException('Invalid credentials.')
        }

        // console.log(`args: ${inspect(args)}`)
        // console.log(`args: ${inspect(args)}`)
        // this.clients.set(client.id, client)
    }

    @SubscribeMessage('report')
    findAll(
        @MessageBody() fusion: CreateFusionDto,
        @ConnectedSocket() client: any
    ) {
        // this.clientService.addData(fusion)
        console.log(`report: ${inspect(fusion)}`)
        client.emit('report', 'ok')
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
