import {
    Body,
    Controller,
    Get,
    HttpCode,
    HttpStatus,
    Post,
    Request,
    Res,
    UseGuards
} from '@nestjs/common'
import { ResultUtil } from '@server-octopus/shared'
import {
    UserDevice,
    UserLoginDto,
    UserRegisterDto
} from '@server-octopus/types'
import { Response } from 'express'
import { kCookieAccessToken, kCookieRefreshToken } from './auth.const'
import { AuthService } from './auth.service'
import { RefreshGuard } from './guard/refresh.guard'
import { StatusGuard } from './guard/status.guard'
import { UserDeviceGuard } from './guard/ud.guard'

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
            return ResultUtil.error(`Register Failed: ${e.message}`)
        }
    }

    @UseGuards(UserDeviceGuard)
    @HttpCode(HttpStatus.OK)
    @Post('login')
    async signIn(
        @Request() req: Request & { ud: UserDevice },
        @Body() signInfoDto: UserLoginDto,
        @Res({ passthrough: true }) res: Response
    ) {
        try {
            const result = await this.authService.signIn(signInfoDto, req.ud)
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

    @UseGuards(RefreshGuard)
    @HttpCode(HttpStatus.OK)
    @Post('refresh')
    async refresh(
        @Request() req: Request & { access_token: string },
        @Res({ passthrough: true }) res: Response
    ) {
        try {
            if (req.access_token) {
                const expiration = await this.authService.getTokenExpiration()
                res.cookie(kCookieAccessToken, req.access_token, {
                    httpOnly: true,
                    sameSite: 'strict',
                    expires: new Date(
                        new Date().getTime() +
                            expiration.accessExpiration * 1000
                    )
                })
                return ResultUtil.ok()
            }
            return ResultUtil.error('Refresh Failed')
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @HttpCode(HttpStatus.OK)
    @Post('logout')
    async logout(@Res({ passthrough: true }) res: Response) {
        try {
            res.clearCookie(kCookieAccessToken)
            res.clearCookie(kCookieRefreshToken)
            return ResultUtil.ok()
        } catch (e) {
            return ResultUtil.error(e.message)
        }
    }

    @UseGuards(StatusGuard)
    @HttpCode(HttpStatus.OK)
    @Get('check')
    async checkLoginStatus() {
        return ResultUtil.ok()
    }
}
