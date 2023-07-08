import { Inject, Injectable } from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import { CreateUser, Result, UserVo } from '@server-octopus/types'
import { kStorageService, kUserCreateEvent } from '@server-octopus/shared'
import { firstValueFrom } from 'rxjs'

@Injectable()
export class UserService {
    constructor(
        @Inject(kStorageService)
        private client: ClientProxy
    ) {}

    async createUser(user: CreateUser): Promise<UserVo> {
        const { success, message, data } = await firstValueFrom(
            this.client.send<Result<UserVo>>(kUserCreateEvent, user)
        )
        if (!success || !data) {
            throw new Error(message)
        }
        return data
    }
}
