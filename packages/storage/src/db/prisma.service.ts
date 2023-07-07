import { INestApplication, Injectable, OnModuleInit } from '@nestjs/common'
import { PrismaClient } from '@prisma/client'
import {
    ClientDto,
    DeviceDto,
    DiskDetailDto,
    NetworkInfoDto
} from '@server-octopus/types'

@Injectable()
export class PrismaService extends PrismaClient implements OnModuleInit {
    async onModuleInit() {
        await this.$connect()
    }

    async enableShutdownHooks(app: INestApplication) {
        this.$on('beforeExit', async () => {
            await app.close()
        })
    }

    async getClientByClientId(clientId: string): Promise<ClientDto> {
        const {
            name,
            device,
            user_id: userId
        } = await this.client.findUnique({
            where: {
                client_id: clientId
            },
            include: {
                device: {
                    include: {
                        network: true,
                        disk: true
                    }
                }
            }
        })
        return {
            name,
            device: {
                name: device.name,
                hostname: device.hostname,
                kernel: device.kernel,
                cpu_num: device.cpu_num,
                brand: device.brand,
                frequency: device.frequency,
                vendor: device.vendor,
                memory: device.memory,
                swap: device.swap,
                version: device.version,
                disk: device.disk.map<DiskDetailDto>((disk) => ({
                    disk_type: disk.type,
                    device_name: disk.name,
                    file_system: disk.file_system,
                    total_space: disk.total,
                    available_space: disk.available,
                    is_removable: disk.removeable
                })),
                network: device.network.map<NetworkInfoDto>((network) => ({
                    name: network.name,
                    mac: network.mac,
                    rx: network.rx,
                    tx: network.tx
                }))
            } as DeviceDto,
            clientId,
            userId
        } as ClientDto
    }
}
