import { Inject, Injectable } from '@nestjs/common'
import { kStorageService, kUserCreateEvent } from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import {
    CreateUser,
    Result,
    UserRegisterDto,
    UserVo
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import * as bcrypt from 'bcrypt'

@Injectable()
export class UserService {
    constructor(@Inject(kStorageService) private client: ClientProxy) {}

    async register(data: UserRegisterDto): Promise<UserVo> {
        // hash pass
        const salt = parseInt(await bcrypt.genSalt())
        const hashPass = await bcrypt.hash(data.password, salt)

        const {
            success,
            message,
            data: user
        } = await firstValueFrom(
            this.client.send<Result<UserVo>, CreateUser>(kUserCreateEvent, {
                ...data,
                password: hashPass,
                salt
            })
        )
        if (!success || !user) {
            throw new Error(message)
        }
        return user
    }
}
