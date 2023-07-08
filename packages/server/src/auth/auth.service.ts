import {
    Inject,
    Injectable,
    Logger,
    UnauthorizedException
} from '@nestjs/common'
import { JwtService } from '@nestjs/jwt'
import { ConfigService } from '@nestjs/config'
import { UserService } from 'src/user/user.service'
import { kStorageService, kUserVerify } from '@server-octopus/shared'
import { ClientProxy } from '@nestjs/microservices'
import { VertifyUserDto, VertifyUserResult } from '@server-octopus/types'
import { firstValueFrom } from 'rxjs'
import { inspect } from 'util'

@Injectable()
export class AuthService {
    private readonly logger = new Logger(AuthService.name)

    constructor(
        private usersService: UserService,
        private jwtService: JwtService,
        private configService: ConfigService,
        @Inject(kStorageService) private client: ClientProxy
    ) {}

    async signIn(pass: string, email?: string, username?: string) {
        if (!email && !username) {
            this.logger.error('email or username is required')
            throw new UnauthorizedException()
        }
        const { success, message, data } = await firstValueFrom(
            this.client.send<VertifyUserResult, VertifyUserDto>(kUserVerify, {
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
        return {
            access_token: await this.jwtService.signAsync(data),
            refresh_token: await this.jwtService.signAsync(data)
        }
    }

    async register(pass: string, email: string, username: string) {
        return this.usersService.createUser({
            email,
            password: pass,
            username
        })
    }

    getSaltRounds() {
        const saltRounds = this.configService.get('SALT_ROUNDS')
        return saltRounds ? parseInt(saltRounds) : 10
    }

    /**
     * unit seconds from .env
     */
    getJwtAccessExpirationTime() {
        const jwtExpirationTime = this.configService.get('JWT_EXPIRATION_TIME')
        return jwtExpirationTime ? parseInt(jwtExpirationTime) : 86400
    }

    /**
     * unit seconds from .env
     */
    getJwtRefreshExpirationTime() {
        const jwtRefreshExpirationTime = this.configService.get(
            'JWT_REFRESH_EXPIRATION_TIME'
        )
        return jwtRefreshExpirationTime
            ? parseInt(jwtRefreshExpirationTime)
            : 86400 * 6
    }
}
