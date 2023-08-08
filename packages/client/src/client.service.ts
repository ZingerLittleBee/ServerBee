import { Inject, Injectable, Logger } from '@nestjs/common'
import { inspect } from 'util'
import {
    kAuthService,
    kClientCreateMsg,
    kClientDeviceUpdateEvent,
    kClientTokenSign,
    kClientTokenValid,
    kFusionAddEvent,
    kStorageService,
    Result
} from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import {
    ClientPayload,
    CreateClientDto,
    CreateClientResult,
    FusionDto,
    RegisterClientDto,
    UpdateDeviceDto
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        @Inject(kAuthService) private authClient: ClientProxy,
        @Inject(kStorageService) private storageClient: ClientProxy
    ) {}

    async registerClient(client: RegisterClientDto) {
        const {
            success,
            message,
            data: clientId
        } = await firstValueFrom(
            this.storageClient.send<Result<string>, UpdateDeviceDto>(
                kClientDeviceUpdateEvent,
                {
                    clientId: client.clientId,
                    device: client.device
                }
            )
        )
        if (!success) {
            this.logger.error(
                `update device: ${inspect(client)} error, message: ${message}`
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

    async create(client: CreateClientDto) {
        const { success, message, data } = await firstValueFrom(
            this.storageClient.send<CreateClientResult, CreateClientDto>(
                kClientCreateMsg,
                client
            )
        )
        if (!success) {
            this.logger.error(
                `create client: ${inspect(client)} error, message: ${message}`
            )
            throw new Error(message)
        }
        return data
    }

    async signToken(userId: string, clientId: string): Promise<string> {
        const { success, message, data } = await firstValueFrom(
            this.authClient.send<Result<string>, ClientPayload>(
                kClientTokenSign,
                { userId, clientId }
            )
        )
        if (!success || !data) {
            this.logger.error(
                `sign token: ${inspect({
                    userId,
                    clientId
                })} error, message: ${message}`
            )
            throw new Error(message)
        }
        return data
    }
}
