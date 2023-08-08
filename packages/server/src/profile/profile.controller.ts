import { Body, Controller, Get, Post, Request, UseGuards } from '@nestjs/common'
import { ProfileService } from './profile.service'
import { AuthGuard } from '../auth/guard/auth.guard'
import { ResultUtil } from '@server-octopus/shared'
import { CreateProfile } from '@server-octopus/types'

@Controller('profile')
export class ProfileController {
    constructor(private readonly profileService: ProfileService) {}

    @Post()
    create(@Body() profile: CreateProfile) {
        return this.profileService.create(profile)
    }

    @UseGuards(AuthGuard)
    @Get()
    async getProfile(@Request() req: Request & { userId: string }) {
        const res = await this.profileService.getByUserId(req['userId'])
        return ResultUtil.ok(res)
    }
}
