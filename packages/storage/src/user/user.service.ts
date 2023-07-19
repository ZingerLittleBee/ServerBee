import { Injectable } from '@nestjs/common'
import {
    CreateUser,
    FindUserDto,
    UserVo,
    VerifyUserParam
} from '@server-octopus/types'
import * as bcrypt from 'bcrypt'
import { PrismaService } from '@/db/prisma.service'
import { ErrorService } from '@/utils/error.util'

@Injectable()
export class UserService {
    constructor(
        private readonly prismaService: PrismaService,
        private readonly errorService: ErrorService
    ) {}

    async createUser(data: CreateUser) {
        try {
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
        } catch (e) {
            const { errorCode, message } = this.errorService.explain(e)
            throw new Error(
                errorCode === 'P2002'
                    ? 'Email or Username already exists'
                    : message
            )
        }
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

    async verifyUser(data: VerifyUserParam): Promise<string> {
        const whereClause: Omit<VerifyUserParam, 'password'> = {}
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
        return user.user_id
    }

    private async comparePassword(pass: string, hash: string) {
        return bcrypt.compare(pass, hash)
    }
}
