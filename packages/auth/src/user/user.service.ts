import { SignType, TokenType } from '@/token/token.const'
import { TokenService } from '@/token/token.service'
import { TokenUtilService } from '@/token/token.util.service'
import { Inject, Injectable, Logger } from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import { kStorageService, kUserCreateEvent } from '@server-octopus/shared'
import {
    CreateUser,
    Result,
    UserPayload,
    UserRegisterDto,
    UserVo
} from '@server-octopus/types'
import * as bcrypt from 'bcrypt'
import { firstValueFrom } from 'rxjs'

@Injectable()
export class UserService {
    private logger = new Logger(UserService.name)

    constructor(
        @Inject(kStorageService) private client: ClientProxy,
        private tokenService: TokenService,
        private tokenUtilService: TokenUtilService
    ) {}

    async register(data: UserRegisterDto): Promise<UserVo> {
        // hash pass
        const hashPass = await bcrypt.hash(
            data.password,
            this.tokenUtilService.getSaltRounds()
        )

        const {
            success,
            message,
            data: user
        } = await firstValueFrom(
            this.client.send<Result<UserVo>, CreateUser>(kUserCreateEvent, {
                ...data,
                password: hashPass
            })
        )
        if (!success || !user) {
            this.logger.error(`Invalid ${kUserCreateEvent} Failed: ${message}`)
            throw new Error(message)
        }
        return user
    }

    async sign(payload: UserPayload) {
        return this.tokenService.signGroup(payload, SignType.user)
    }

    async verify(token: string) {
        return this.tokenService.verify<UserPayload>(
            token,
            TokenType.userAccess
        )
    }

    async refreshToken(token: string): Promise<string> {
        const payload = await this.verify(token)
        const { accessOptions } = this.tokenService.getOptions(SignType.user)
        return this.tokenService.sign(payload, payload.userId, accessOptions)
    }
}
