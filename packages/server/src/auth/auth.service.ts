import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { ClientProxy } from '@nestjs/microservices'
import {
    defaultAccessExpiration,
    defaultRefreshExpiration,
    kAuthService,
    kStorageService,
    kUserAccessTokenValid,
    kUserDeviceCreate,
    kUserRegister,
    kUserTokenExpirationGet,
    kUserTokenRefresh,
    kUserTokenSign,
    kUserVerify
} from '@server-octopus/shared'
import {
    CreateUdParam,
    CreateUdResult,
    UserDevice,
    UserLoginDto,
    UserPayload,
    UserRegisterDto,
    UserRegisterResult,
    UserTokenExpiration,
    UserTokenExpirationResult,
    UserTokenRefreshParam,
    UserTokenRefreshResult,
    UserTokenSignResult,
    UserTokenValidParam,
    UserTokenValidResult,
    VerifyUserParam,
    VerifyUserResult
} from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { inspect } from 'util'

@Injectable()
export class AuthService {
    private readonly logger = new Logger(AuthService.name)
    private tokenExpiration: UserTokenExpiration

    constructor(
        @Inject(kAuthService) private authClient: ClientProxy,
        @Inject(kStorageService) private storageClient: ClientProxy
    ) {}

    async signIn(userInfo: UserLoginDto, userDevice: UserDevice) {
        if (!userInfo.email && !userInfo.username) {
            this.logger.error('Email or Username is required')
            throw new UnauthorizedException('Email or Username is required')
        }
        const userId = await this.verifyUser(userInfo)
        const clientId = await this.createUserDevice({ userId, ...userDevice })

        const {
            success: signSuccess,
            data: signData,
            message: signMsg
        } = await firstValueFrom(
            this.authClient.send<UserTokenSignResult, UserPayload>(
                kUserTokenSign,
                {
                    userId,
                    clientId
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
        if (this.tokenExpiration) {
            return this.tokenExpiration
        }
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserTokenExpirationResult>(
                kUserTokenExpirationGet,
                {}
            )
        )
        if (!success || !data) {
            this.logger.error(`Get Token Expiration Error: ${message}`)
            this.tokenExpiration = {
                accessExpiration: defaultAccessExpiration,
                refreshExpiration: defaultRefreshExpiration
            }
        } else {
            this.tokenExpiration = data
        }
        return this.tokenExpiration
    }

    async refreshToken(token: string) {
        const { success, data, message } = await firstValueFrom(
            this.authClient.send<UserTokenRefreshResult, UserTokenRefreshParam>(
                kUserTokenRefresh,
                {
                    refreshToken: token
                }
            )
        )
        if (!success || !data) {
            this.logger.error(`Invoke ${kUserTokenRefresh} Error: ${message}`)
            throw new Error('Refresh Token Error')
        }
        return data
    }

    async registerUserDevice() {}

    async verifyUser(userInfo: UserLoginDto) {
        const { success, message, data } = await firstValueFrom(
            this.authClient.send<VerifyUserResult, VerifyUserParam>(
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
        return data.userId
    }

    async checkAccessToken(token: string): Promise<boolean> {
        const { success } = await firstValueFrom(
            this.authClient.send<UserTokenValidResult, UserTokenValidParam>(
                kUserAccessTokenValid,
                { token }
            )
        )
        return success
    }

    async createUserDevice(ud: CreateUdParam) {
        const { success, data, message } = await firstValueFrom(
            this.storageClient.send<CreateUdResult, CreateUdParam>(
                kUserDeviceCreate,
                ud
            )
        )
        if (!success || !data) {
            this.logger.error(`Create User Device Error: ${message}`)
            throw new Error('Create User Device Error')
        }
        return data
    }
}
