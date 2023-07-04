import { Injectable, Logger } from '@nestjs/common'
import { CreateClientDto } from './dto/create-client.dto'
import { UpdateClientDto } from './dto/update-client.dto'
import { InfluxService } from '@/influx/influx.service'
import { PrismaService } from '@/db/prisma.service'
import { Prisma } from '@prisma/client'
import { StatusEnum } from '@/enums/status.enum'
import { JwtService } from '@nestjs/jwt'
import { ConfigService } from '@nestjs/config'
import { JwtUtilService } from '@/utils/jwt.util.service'
import { CreateFusionDto } from '@/client/dto/create-fusion.dto'
import { MongoService } from '@/db/mongo.service'
import { RedisService } from '@/db/redis.service'
import { Error } from 'mongoose'
import { formatDataToString } from '@/utils/common.util'
import { CreateDeviceDto } from '@/client/device/dto/create-device.dto'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        private readonly influxService: InfluxService,
        private readonly prismaService: PrismaService,
        private readonly jwtService: JwtService,
        private readonly configService: ConfigService,
        private readonly jwtUtilService: JwtUtilService,
        private readonly mongoService: MongoService,
        private readonly redisService: RedisService
    ) {}

    async registerClient(client: CreateClientDto) {
        // if `clientId` not empty, already registered before.
        // just update `status` and `device` field, if it has.
        let clientId: string
        if (client.clientId) {
            this.logger.verbose(
                `clientId: ${client.clientId} already registered before`
            )
            try {
                await this.mergeClient(client)
                clientId = client.clientId
            } catch (e) {
                this.logger.error(`merge client error: ${e.message}`)
                throw new Error('update client error')
            }
        } else {
            try {
                clientId = await this.create(client)
                this.logger.verbose(
                    `new client, id: ${clientId}, info: ${client}`
                )
            } catch (e) {
                this.logger.error(`create client error: ${e.message}`)
                throw new Error('create client error')
            }
        }
        const token = await this.jwtService.signAsync({
            clientId: clientId,
            userId: client.userId
        })
        await this.redisService.setWithExpire(
            clientId,
            token,
            this.jwtUtilService.getClientAccessExpireTime()
        )
        return token
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
                userId: client.userId
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
                        ...newClient.device,
                        memory: formatDataToString(newClient.device?.memory),
                        swap: formatDataToString(newClient.device?.swap)
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
                        memory: formatDataToString(device.memory),
                        swap: formatDataToString(device.swap)
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
        return this.mongoService.createFusion(fusion)
    }
}
