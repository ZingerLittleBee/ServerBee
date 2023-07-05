import {
    Body,
    Controller,
    Post,
    HttpCode,
    HttpStatus,
    Logger,
    Res
} from '@nestjs/common'
import { AuthService } from './auth.service'
import { hash } from 'bcrypt'
import { Response } from 'express'
import { Result, ResultUtil } from '@app/shared/utils/result.util'
import { ErrorService } from '@app/shared/utils/error.util'

@Controller('auth')
export class AuthController {
    private readonly logger = new Logger(AuthController.name)

    constructor(
        private authService: AuthService,
        private readonly errorService: ErrorService
    ) {}

    @HttpCode(HttpStatus.OK)
    @Post('register')
    async register(
        @Body() signInDto: Record<string, any>
    ): Promise<Result<{ token: string }>> {
        try {
            const hashPass = await hash(
                signInDto.password,
                this.authService.getSaltRounds()
            )
            await this.authService.register(
                hashPass,
                signInDto.email,
                signInDto.username
            )
            return ResultUtil.ok()
        } catch (e) {
            return ResultUtil.error(
                this.errorService.explain(e) ??
                    'Register failed, please try again later'
            )
        }
    }

    @HttpCode(HttpStatus.OK)
    @Post('login')
    async signIn(
        @Body() signInDto: Record<string, any>,
        @Res({ passthrough: true }) res: Response
    ) {
        console.log('login controller')
        try {
            const result = await this.authService.signIn(
                signInDto.password,
                signInDto.email,
                signInDto.username
            )
            res.cookie('access_token', result.access_token, {
                httpOnly: true,
                sameSite: 'strict',
                expires: new Date(
                    new Date().getTime() +
                        this.authService.getJwtAccessExpirationTime() * 1000
                )
            })
            res.cookie('refresh_token', result.refresh_token, {
                httpOnly: true,
                sameSite: 'strict',
                expires: new Date(
                    new Date().getTime() +
                        this.authService.getJwtRefreshExpirationTime() * 1000
                )
            })
            return res.status(200).json(ResultUtil.ok())
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
