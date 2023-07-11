import { Inject, Injectable, Logger } from '@nestjs/common'
import { kRedisEqualEvent, kStorageService } from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { TokenService } from '@/token/token.service'
import { firstValueFrom } from 'rxjs'
import { Result } from '@server-octopus/types'

@Injectable()
export class AppService {
    private readonly logger = new Logger(AppService.name)

    constructor(
        @Inject(kStorageService) private client: ClientProxy,
        private readonly tokenService: TokenService
    ) {}

    async isTokenValid(token: string): Promise<boolean> {
        this.tokenService.checkExpireTime(token)
        const { success, data, message } = await firstValueFrom(
            this.client.send<Result<boolean>>(kRedisEqualEvent, {
                key: this.tokenService.extractClientId(token),
                value: token
            })
        )
        if (!success || !data) {
            this.logger.error(`invoke ${kRedisEqualEvent} error: ${message}`)
            return false
        }
    }
}
