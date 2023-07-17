import {
    Body,
    Controller,
    HttpCode,
    HttpStatus,
    Post,
    Res
} from '@nestjs/common'
import { ResultUtil } from '@server-octopus/shared'
import { UserLoginDto, UserRegisterDto } from '@server-octopus/types'
import { Response } from 'express'
import { kCookieAccessToken, kCookieRefreshToken } from './auth.const'
import { AuthService } from './auth.service'

@Controller('auth')
export class AuthController {
    constructor(private authService: AuthService) {}

    @HttpCode(HttpStatus.OK)
    @Post('register')
    async register(@Body() registerDto: UserRegisterDto) {
        try {
            await this.authService.register(registerDto)
            return ResultUtil.ok()
        } catch (e) {
            return ResultUtil.error(`Register failed: ${e.message}`)
        }
    }

    @HttpCode(HttpStatus.OK)
    @Post('login')
    async signIn(
        @Body() signInfoDto: UserLoginDto,
        @Res({ passthrough: true }) res: Response
    ) {
        try {
            const result = await this.authService.signIn(signInfoDto)
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
