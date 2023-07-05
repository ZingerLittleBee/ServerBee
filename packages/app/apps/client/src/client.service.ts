import { Injectable, Logger } from '@nestjs/common'
import { CreateClientDto } from './dto/create-client.dto'
import { Prisma } from '@prisma/client'
import { JwtService } from '@nestjs/jwt'
import { Error } from 'mongoose'
import { inspect } from 'util'
import { PrismaService } from '@app/shared/db/prisma.service'
import { JwtUtilService } from '@app/shared/utils/jwt.util.service'
import { MongoService } from '@app/shared/db/mongo.service'
import { RedisService } from '@app/shared/db/redis.service'
import { CreateClientEntity } from '@app/client/entity/create-client.entity'
import { UpdateClientEntity } from '@app/client/entity/update-client.entity'
import { StatusEnum } from '@app/server/enums/status.enum'
import { CreateFusionDto } from '@app/client/dto/create-fusion.dto'

@Injectable()
export class ClientService {
    private readonly logger = new Logger(ClientService.name)

    constructor(
        private readonly prismaService: PrismaService,
        private readonly jwtService: JwtService,
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
                clientId = await this.create(
                    new CreateClientEntity({
                        name: client.name,
                        device: client.device.toDeviceEntity(),
                        userId: client.userId,
                        clientId: client.clientId
                    })
                )
                this.logger.verbose(
                    `new client, id: ${clientId}, info: ${inspect(client)}`
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
            `clientId: ${client.clientId}, query client data: ${inspect(
                rawClient
            )}`
        )
        if (rawClient) {
            const newClient: UpdateClientEntity = {
                name: client.name,
                userId: client.userId,
                clientId: client.clientId
            }
            if (client.device && rawClient.device) {
                const { id, created_at, updated_at, client_id, ...other } =
                    rawClient.device
                newClient.device = {
                    ...other,
                    ...client.device.toDeviceEntity()
                }
            }
            this.logger.verbose(
                `clientId: ${newClient.clientId}, update client data: ${inspect(
                    newClient
                )}`
            )
            await this.update(newClient)
        } else {
            this.logger.error(`clientId: ${client.clientId} not found`)
            throw new Error(`Client not found`)
        }
    }

    async update(newClient: UpdateClientEntity) {
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
                    } as Prisma.DeviceUpdateInput
                }
            }
        })
    }

    /**
     * @return ClientId, not db primary key
     * @param createClient
     */
    async create(createClient: CreateClientEntity) {
        const device = createClient.device
        const client = await this.prismaService.client.create({
            data: {
                name: createClient.name,
                status: StatusEnum.ACTIVE,
                user_id: createClient.userId,
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
        return this.mongoService.createFusion(fusion)
    }
}
