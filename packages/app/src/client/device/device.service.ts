import { PrismaService } from '@/db/prisma.service'
import { Injectable } from '@nestjs/common'

@Injectable()
export class DeviceService {
    constructor(private readonly prismaService: PrismaService) {}
}
