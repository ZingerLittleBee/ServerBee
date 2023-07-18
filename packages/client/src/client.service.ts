import { Inject, Injectable, Logger } from '@nestjs/common'
import { inspect } from 'util'
import {
    kAuthService,
    kClientTokenSign,
    kClientTokenValid,
    kClientUpsertEvent,
    kFusionAddEvent,
    kStorageService,
    Result
} from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { CreateClient, FusionDto } from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { convertFormatDataToString } from '@/util'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        @Inject(kAuthService) private authClient: ClientProxy,
        @Inject(kStorageService) private storageClient: ClientProxy
    ) {}

    async registerClient(client: CreateClient) {
        const c = convertFormatDataToString(client)
        const {
            success,
            message,
            data: clientId
        } = await firstValueFrom(
            this.storageClient.send<Result<string>>(kClientUpsertEvent, c)
        )
        if (!success) {
            this.logger.error(
                `upsert client: ${inspect(client)} error, message: ${message}`
            )
            throw new Error(message)
        }
        const {
            success: signSuccess,
            message: signMsg,
            data: signData
        } = await firstValueFrom(
            this.storageClient.send<Result<string>>(kClientTokenSign, {
                clientId,
                userId: client.userId
            })
        )
        if (!signSuccess || !signData) {
            this.logger.error(
                `Sign token: ${inspect(client)} error, message: ${signMsg}`
            )
            throw new Error('Sign Token Error')
        }
        return signData
    }

    async isTokenValid(token: string) {
        const { success, message, data } = await firstValueFrom(
            this.authClient.send<Result<boolean>>(kClientTokenValid, { token })
        )
        if (!success) {
            this.logger.error(`token: ${token} invalid, message: ${message}`)
            return false
        }
        return data
    }

    async addData(fusion: FusionDto) {
        this.storageClient.emit(kFusionAddEvent, fusion)
    }
}
