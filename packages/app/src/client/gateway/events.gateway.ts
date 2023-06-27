import {
    MessageBody,
    OnGatewayConnection,
    SubscribeMessage,
    WebSocketGateway,
    WebSocketServer
} from '@nestjs/websockets'
import { Injectable } from '@nestjs/common'
import { ClientService } from '@/client/client.service'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'
import { Server, Socket } from 'socket.io'
import { inspect } from 'util'

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

    constructor(private readonly clientService: ClientService) {}

    handleConnection(client: Socket, ...args: any[]) {
        // console.log(`handleConnection: ${inspect(client.handshake)}`)
        // console.log(`args: ${inspect(args)}`)
        // this.clients.set(client.id, client)
    }

    @SubscribeMessage('report')
    findAll(@MessageBody() fusion: CreateFusionDto) {
        this.clientService.addData(fusion)
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
