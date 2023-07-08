import { Injectable } from '@nestjs/common'
import {
    AuthPayload,
    CreateUser,
    FindUserDto,
    UserVo,
    VertifyUserDto
} from '@server-octopus/types'
import * as bcrypt from 'bcrypt'
import { PrismaService } from '@/db/prisma.service'

@Injectable()
export class UserService {
    constructor(private readonly prismaService: PrismaService) {}

    async createUser(data: CreateUser) {
        const user = await this.prismaService.user.create({
            data: {
                ...data
            }
        })
        return {
            userId: user.user_id,
            username: user.username,
            email: user.email
        } as UserVo
    }

    async findUser(data: FindUserDto) {
        const findUser: Omit<FindUserDto, 'userId'> & { user_id?: string } = {}
        if (data.userId) findUser.user_id = data.userId
        if (data.username) findUser.username = data.username
        if (data.email) findUser.email = data.email
        const user = await this.prismaService.user.findUnique({
            where: {
                ...data
            }
        })
        return {
            userId: user.user_id,
            username: user.username,
            email: user.email
        } as UserVo
    }

    async verifyUser(data: VertifyUserDto): Promise<AuthPayload> {
        const whereClause: Omit<VertifyUserDto, 'password'> = {}
        if (data.email) whereClause.email = data.email
        if (data.username) whereClause.username = data.username
        const user = await this.prismaService.user.findUnique({
            where: {
                ...whereClause
            }
        })
        if (!user) throw new Error('user not found')
        const isMatch = await this.comparePassword(data.password, user.password)
        if (!isMatch) throw new Error('password is incorrect')
        return {
            userId: user.user_id
        }
    }

    private async comparePassword(pass: string, hash: string) {
        return bcrypt.compare(pass, hash)
    }
}
