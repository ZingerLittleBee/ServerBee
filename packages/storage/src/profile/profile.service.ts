import { Injectable } from '@nestjs/common'
import { CreateProfile, ProfileVo } from '@server-octopus/types'
import { PrismaService } from '@/db/prisma.service'

@Injectable()
export class ProfileService {
    constructor(private prismaService: PrismaService) {}

    async getProfileByUserId(userId: string): Promise<ProfileVo> {
        const val = await this.prismaService.profile.findUnique({
            where: {
                user_id: userId
            }
        })
        return {
            name: val?.name,
            avatar: val?.avatar,
            description: val?.description
        }
    }

    async createProfile(profile: CreateProfile) {
        const res = await this.prismaService.profile.create({
            data: {
                name: profile.name,
                avatar: profile.avatar,
                description: profile.description,
                user_id: profile.userId
            }
        })
        return {
            name: res.name,
            avatar: res.avatar,
            description: res.description
        } as ProfileVo
    }
}
