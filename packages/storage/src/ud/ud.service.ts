import { Injectable } from '@nestjs/common'
import { PrismaService } from '@/db/prisma.service'
import { CreateUdParam } from '@server-octopus/types'
import { ErrorService } from '@/utils/error.util'

@Injectable()
export class UdService {
    constructor(
        private readonly prismaService: PrismaService,
        private readonly errorService: ErrorService
    ) {}

    async createUd(data: CreateUdParam): Promise<string> {
        try {
            const { ud_id: id } = await this.prismaService.ud.create({
                data: {
                    ...data,
                    user_id: data.userId
                }
            })
            return id
        } catch (e) {
            this.errorService.explain(e)
            throw new Error('Failed to create ud')
        }
    }
}
