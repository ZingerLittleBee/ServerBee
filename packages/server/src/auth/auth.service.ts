import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import {
    kAuthService,
    kUserRegister,
    kUserTokenExpirationGet,
    kUserTokenSign,
    kUserVerify
} from '@server-octopus/shared'
import {
    UserLoginDto,
    UserPayload,
    UserRegisterDto,
    UserRegisterResult,
    UserTokenExpiration,
    UserTokenExpirationResult,
    UserTokenSignResult,
    VerifyUserDto,
    VerifyUserResult
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { inspect } from 'util'
import { defaultAccessExpiration, defaultRefreshExpiration } from './auth.const'

@Injectable()
export class AuthService {
    private readonly logger = new Logger(AuthService.name)

    constructor(@Inject(kAuthService) private authClient: ClientProxy) {}

    async signIn(userInfo: UserLoginDto) {
        if (!userInfo.email && !userInfo.username) {
            this.logger.error('email or username is required')
            throw new UnauthorizedException()
        }
        const { success, message, data } = await firstValueFrom(
            this.authClient.send<VerifyUserResult, VerifyUserDto>(
                kUserVerify,
                userInfo
            )
        )
        if (!success || !data) {
            this.logger.error(`verify user error: ${message}`)
            throw new UnauthorizedException(`username or password is incorrect`)
        }
        this.logger.verbose(
            `username: ${userInfo.username}, email: ${
                userInfo.email
            }, verified, payload: ${inspect(data)}`
        )

        const {
            success: signSuccess,
            data: signData,
            message: signMsg
        } = await firstValueFrom(
            this.authClient.send<UserTokenSignResult, UserPayload>(
                kUserTokenSign,
                {
                    userId: data.userId
                }
            )
        )
        if (!signSuccess || !signData) {
            this.logger.error(`Sign Error: ${signMsg}`)
            throw new UnauthorizedException('Sign Error')
        }
        return {
            access_token: signData.accessToken,
            refresh_token: signData.refreshToken
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

    async refreshToken() {}
}
