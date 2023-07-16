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
import { ClientService } from '@/client.service'
import { EventsService } from '@/gateway/events.service'
import { Fusion } from '@server-octopus/types'

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

    async handleConnection(client: any, ...args: any[]) {
        // console.log(`handleConnection: ${inspect(client)}`)
        try {
            const token = this.eventsService.extractBearerTokenFromRawHeaders(
                args[0].rawHeaders
            )
            // parse token
            const clientId = await this.eventsService.extractClientIdFromToken(
                token
            )
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
    reportFromClient(
        @MessageBody() fusion: Fusion,
        @ConnectedSocket() client: any
    ) {
        // this.clientService.addData(fusion)
        console.log(`report: ${inspect(fusion)}`)
        client.emit('report', 'ok')
    }
}
