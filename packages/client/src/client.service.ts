import { Inject, Injectable, Logger } from '@nestjs/common'
import { CreateClientDto } from './dto/create-client.dto'
import { JwtService } from '@nestjs/jwt'
import { Error } from 'mongoose'
import { inspect } from 'util'
import {
    JwtUtilService,
    kClientUpsertEvent,
    kFusionAddEvent,
    kJwtCreatedEvent,
    Result
} from '@server-octopus/shared'
import { CreateFusionDto } from '@/dto/create-fusion.dto'
import { ClientProxy } from '@nestjs/microservices'
import { EventJwtCreated } from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        private readonly jwtService: JwtService,
        private readonly jwtUtilService: JwtUtilService,
        @Inject('STORAGE_SERVICE') private client: ClientProxy
    ) {}

    async registerClient(client: CreateClientDto) {
        const {
            success,
            message,
            data: clientId
        } = await firstValueFrom(
            this.client.send<Result<string>>(kClientUpsertEvent, client)
        )
        if (!success) {
            this.logger.error(
                `upsert client: ${inspect(client)} error, message: ${message}`
            )
            throw new Error(message)
        }
        const token = await this.jwtService.signAsync({
            clientId: clientId,
            userId: client.userId
        })
        const jwtCreated: EventJwtCreated = {
            key: clientId,
            value: token,
            expire: this.jwtUtilService.getClientAccessExpireTime()
        }
        this.client.emit<unknown, EventJwtCreated>(kJwtCreatedEvent, jwtCreated)
        return token
    }

    async addData(fusion: CreateFusionDto) {
        this.client.emit(kFusionAddEvent, fusion)
    }
}
