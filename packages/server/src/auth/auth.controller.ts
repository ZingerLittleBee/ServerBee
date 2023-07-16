import {
    Body,
    Controller,
    HttpCode,
    HttpStatus,
    Post,
    Res
} from '@nestjs/common'
import { AuthService } from './auth.service'
import { hash } from 'bcrypt'
import { Response } from 'express'
import { Result, ResultUtil } from '@server-octopus/shared'
import { kCookieAccessToken, kCookieRefreshToken } from './auth.const'

@Controller('auth')
export class AuthController {
    constructor(private authService: AuthService) {}

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
            return ResultUtil.error('Register failed, please try again later')
        }
    }

    @HttpCode(HttpStatus.OK)
    @Post('login')
    async signIn(
        @Body() signInDto: Record<string, any>,
        @Res({ passthrough: true }) res: Response
    ) {
        try {
            const result = await this.authService.signIn(
                signInDto.password,
                signInDto.email,
                signInDto.username
            )
            const expiration = await this.authService.getTokenExpiration()
            res.cookie(kCookieAccessToken, result.access_token, {
                httpOnly: true,
                sameSite: 'strict',
                expires: new Date(
                    new Date().getTime() + expiration.accessExpiration * 1000
                )
            })
            res.cookie(kCookieRefreshToken, result.refresh_token, {
                httpOnly: true,
                sameSite: 'strict',
                expires: new Date(
                    new Date().getTime() + expiration.refreshExpiration * 1000
                )
            })
            return res.status(200).json(ResultUtil.ok())
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }
}
