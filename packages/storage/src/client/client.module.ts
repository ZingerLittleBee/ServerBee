import { Module } from '@nestjs/common'
import { ClientService } from './client.service'
import { ClientController } from './client.controller'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule],
    controllers: [ClientController],
    providers: [ClientService]
})
export class ClientModule {}
