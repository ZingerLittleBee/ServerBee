import { Inject, Injectable } from '@nestjs/common'
import {
    kProfileCreateEvent,
    kProfileGetByUserIdEvent,
    kStorageService
} from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { firstValueFrom } from 'rxjs'
import { CreateProfile, ProfileVo, Result } from '@server-octopus/types'

@Injectable()
export class ProfileService {
    constructor(@Inject(kStorageService) private client: ClientProxy) {}

    async create(profile: CreateProfile) {
        const { success, data, message } = await firstValueFrom(
            this.client.send<Result<ProfileVo>>(kProfileCreateEvent, profile)
        )
        if (!success) {
            throw new Error(message)
        }
        return data
    }

    async getByUserId(userId: string) {
        const { success, data, message } = await firstValueFrom(
            this.client.send<Result<ProfileVo>>(
                kProfileGetByUserIdEvent,
                userId
            )
        )
        if (!success) {
            throw new Error(message)
        }
        return data
    }
}
