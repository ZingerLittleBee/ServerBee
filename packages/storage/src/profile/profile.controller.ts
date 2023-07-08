import { Controller } from '@nestjs/common'
import { ProfileService } from '@/profile/profile.service'
import { MessagePattern } from '@nestjs/microservices'
import {
    kProfileCreateEvent,
    kProfileGetByUserIdEvent
} from '@server-octopus/shared'
import { CreateProfile, ProfileVo, Result } from '@server-octopus/types'
import { ResultUtil } from '@/utils/result.util'
import { ErrorUtil } from '@/db/error.util'

@Controller()
export class ProfileController {
    constructor(
        private readonly errorUtil: ErrorUtil,
        private readonly profileService: ProfileService
    ) {}

    @MessagePattern(kProfileGetByUserIdEvent)
    async getProfileByUserId(userId: string): Promise<Result<ProfileVo>> {
        try {
            return ResultUtil.ok(
                await this.profileService.getProfileByUserId(userId)
            )
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }

    @MessagePattern(kProfileCreateEvent)
    async createProfile(profile: CreateProfile): Promise<Result<ProfileVo>> {
        try {
            return ResultUtil.ok(
                await this.profileService.createProfile(profile)
            )
        } catch (e) {
            return ResultUtil.error(this.errorUtil.explain(e))
        }
    }
}
