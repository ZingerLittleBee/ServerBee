import { Module } from '@nestjs/common'
import { SharedModule } from '@app/shared'
import { ClientController } from '@app/client/client.controller'
import { ClientService } from '@app/client/client.service'
import { EventsGateway } from '@app/client/gateway/events.gateway'
import { EventsService } from '@app/client/gateway/events.service'
import { JwtModule } from "@nestjs/jwt";

@Module({
    imports: [SharedModule,JwtModule.register({})],
    controllers: [ClientController],
    providers: [ClientService, EventsGateway, EventsService]
})
export class ClientModule {}
