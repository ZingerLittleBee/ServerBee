import { CreateDeviceEntity } from '@/client/device/entity/create-device.entity'

export class CreateClientEntity {
    name?: string
    device: CreateDeviceEntity
    userId?: string
    clientId?: string

    constructor(client: {
        name?: string
        device: CreateDeviceEntity
        userId?: string
        clientId?: string
    }) {
        this.device = client.device
        this.name = client.name
        this.userId = client.userId
        this.clientId = client.clientId
    }
}
