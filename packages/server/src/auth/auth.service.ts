import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ConfigService } from '@nestjs/config'
import { ClientProxy } from '@nestjs/microservices'
import {
    kAuthService,
    kUserRegister,
    kUserTokenExpirationGet,
    kUserTokenSign,
    kUserVerify
} from '@server-octopus/shared'
import {
    SignResult,
    UserPayload,
    UserRefreshPayload,
    UserRegisterDto,
    UserRegisterResult,
    UserTokenExpiration,
    UserTokenExpirationResult,
    VerifyUserDto,
    VerifyUserResult
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { UserService } from 'src/user/user.service'
import { inspect } from 'util'
import { defaultAccessExpiration, defaultRefreshExpiration } from './auth.const'

@Injectable()
export class AuthService {
    private readonly logger = new Logger(AuthService.name)

    constructor(
        private usersService: UserService,
        private configService: ConfigService,
        @Inject(kAuthService) private authClient: ClientProxy
    ) {}

    async signIn(pass: string, email?: string, username?: string) {
        if (!email && !username) {
            this.logger.error('email or username is required')
            throw new UnauthorizedException()
        }
        const { success, message, data } = await firstValueFrom(
            this.authClient.send<VerifyUserResult, VerifyUserDto>(kUserVerify, {
                email,
                username,
                password: pass
            })
        )
        if (!success || !data) {
            this.logger.error(`verify user error: ${message}`)
            throw new UnauthorizedException(`username or password is incorrect`)
        }
        this.logger.verbose(
            `username: ${username}, email: ${email}, verified, payload: ${inspect(
                data
            )}`
        )

        const { success: accessSignSuccess, data: accessSignData } =
            await firstValueFrom(
                this.authClient.send<SignResult, UserPayload>(kUserTokenSign, {
                    userId: data.userId
                })
            )

        const { success: refreshSignSuccess, data: refreshSignData } =
            await firstValueFrom(
                this.authClient.send<SignResult, UserRefreshPayload>(
                    kUserTokenSign,
                    {
                        email,
                        username
                    }
                )
            )
        if (
            !accessSignSuccess ||
            !refreshSignSuccess ||
            !accessSignData ||
            !refreshSignData
        ) {
            this.logger.error(
                `Sign Error: ${accessSignData}, ${refreshSignData}`
            )
            throw new UnauthorizedException('Sign Error')
        }
        return {
            access_token: accessSignData,
            refresh_token: refreshSignData
        }
    }

    async register(userDto: UserRegisterDto) {
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserRegisterResult, UserRegisterDto>(
                kUserRegister,
                userDto
            )
        )
        if (!success || !data) {
            throw new Error(message)
        }
    }

    async getTokenExpiration(): Promise<UserTokenExpiration> {
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserTokenExpirationResult>(
                kUserTokenExpirationGet,
                {}
            )
        )
        if (!success || !data) {
            this.logger.error(`Get Token Expiration Error: ${message}`)
            return {
                accessExpiration: defaultAccessExpiration,
                refreshExpiration: defaultRefreshExpiration
            }
        }
        return data
    }
}
