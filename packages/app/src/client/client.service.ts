import { Injectable, Logger } from '@nestjs/common'
import { CreateClientDto } from './dto/create-client.dto'
import { UpdateClientDto } from './dto/update-client.dto'
import { InfluxService } from '@/influx/influx.service'
import { PrismaService } from '@/db/prisma.service'
import { Prisma } from '@prisma/client'
import { StatusEnum } from '@/enums/status.enum'
import { InjectRedis } from '@liaoliaots/nestjs-redis'
import Redis from 'ioredis'
import { JwtService } from '@nestjs/jwt'
import { ConfigService } from '@nestjs/config'
import { kClientAccessExpireTime } from '@/utils/jwt.const'
import { JwtUtilService } from '@/utils/jwt.util.service'
import { Result, ResultUtil } from '@/utils/ResultUtil'
import { InjectModel } from '@nestjs/mongoose'
import { Fusion } from '@/client/schemas/fusion.schema'
import { Model } from 'mongoose'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        private readonly influxService: InfluxService,
        private readonly prismaService: PrismaService,
        private readonly jwtService: JwtService,
        private readonly configService: ConfigService,
        private readonly jwtUtilService: JwtUtilService,
        @InjectRedis() private readonly redis: Redis,
        @InjectModel(Fusion.name) private fusionModel: Model<Fusion>
    ) {}

    async registerClient(client: CreateClientDto) {
        // if `clientId` not empty, already registered before.
        // just update `status` and `device` field, if it has.
        if (client.clientId) {
            this.logger.verbose(
                `clientId: ${client.clientId} already registered before`
            )
            await this.mergeClient(client)
        } else {
            const clientId = await this.create(client)
            this.logger.verbose(`new client, id: ${clientId}, info: ${client}`)
            const token = await this.jwtService.signAsync({
                clientId: clientId
            })
            this.redis.set(
                clientId,
                token,
                'EX',
                this.jwtUtilService.getClientAccessExpireTime()
            )
            return token
        }
    }

    private async mergeClient(client: CreateClientDto) {
        const rawClient = await this.prismaService.client.findUnique({
            where: {
                client_id: client.clientId
            },
            include: {
                device: true
            }
        })
        this.logger.verbose(
            `clientId: ${client.clientId}, query client data: ${rawClient}`
        )
        if (rawClient) {
            const newClient: UpdateClientDto = {
                name: client.name ?? rawClient.name,
                userId: client.userId ?? rawClient.user_id
            }
            if (client.device && rawClient.device) {
                const { id, created_at, updated_at, client_id, ...other } =
                    rawClient.device
                newClient.device = { ...other, ...client.device }
            }
            this.logger.verbose(
                `clientId: ${newClient.clientId}, update client data: ${newClient}`
            )
            await this.update(newClient)
        } else {
            this.logger.error(`clientId: ${client.clientId} not found`)
            throw new Error(`Client not found`)
        }
    }

    async update(newClient: UpdateClientDto) {
        return this.prismaService.client.update({
            where: {
                client_id: newClient.clientId
            },
            data: {
                name: newClient.name,
                status: StatusEnum.ACTIVE,
                user: {
                    connect: {
                        user_id: newClient.userId
                    }
                },
                device: {
                    update: {
                        ...newClient.device
                    }
                }
            }
        })
    }

    /**
     * @param createClientDto
     * @return ClientId, not db primary key
     */
    async create(createClientDto: CreateClientDto) {
        const device = createClientDto.device
        const client = await this.prismaService.client.create({
            data: {
                name: createClientDto.name,
                status: StatusEnum.ACTIVE,
                user: {
                    connect: {
                        user_id: createClientDto.userId
                    }
                },
                device: {
                    create: {
                        hostname: device.hostname,
                        kernel: device.kernel,
                        cpu_num: device.cpu_num,
                        brand: device.brand,
                        frequency: device.frequency,
                        vendor: device.vendor,
                        memory: device.memory,
                        swap: device.swap
                    }
                }
            } as Prisma.ClientCreateInput
        })
        this.logger.verbose(`client: ${client} created`)
        return client.client_id
    }

    findAll() {
        return `This action returns all client`
    }

    findOne(id: number) {
        return `This action returns a #${id} client`
    }

    remove(id: number) {
        return `This action removes a #${id} client`
    }

    async addData(fusion: CreateFusionDto) {
        const createFusion = new this.fusionModel(fusion)
        return createFusion.save()
    }
}
