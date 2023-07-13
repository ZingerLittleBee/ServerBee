import { Inject, Injectable, Logger } from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { Error } from 'mongoose'
import { inspect } from 'util'
import {
    JwtUtilService,
    kClientUpsertEvent,
    kFusionAddEvent,
    kJwtCreatedEvent,
    kStorageService,
    kTokenVerify,
    Result
} from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { CreateClient, EventJwtCreated, FusionDto } from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { convertFormatDataToString } from '@/util'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        private readonly jwtService: JwtService,
        private readonly jwtUtilService: JwtUtilService,
        @Inject(kStorageService) private client: ClientProxy
    ) {}

    async registerClient(client: CreateClient) {
        const c = convertFormatDataToString(client)
        const {
            success,
            message,
            data: clientId
        } = await firstValueFrom(
            this.client.send<Result<string>>(kClientUpsertEvent, c)
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

    async verifyToken(token: string) {
        const { success, message, data } = await firstValueFrom(
            this.client.send<Result<boolean>>(kTokenVerify, { token })
        )
        if (!success) {
            this.logger.error(
                `check token: ${token} error, message: ${message}`
            )
            return false
        }
        return data
    }

    async addData(fusion: FusionDto) {
        this.client.emit(kFusionAddEvent, fusion)
    }
}
