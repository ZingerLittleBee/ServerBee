import { Module } from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientController } from './client.controller'
import { TokenModule } from '@/token/token.module'

@Module({
    imports: [TokenModule],
    controllers: [ClientController],
    providers: [ClientService]
})
export class ClientModule {}
