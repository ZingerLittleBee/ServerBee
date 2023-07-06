import { CreateDeviceEntity } from '@/device/entity/create-device.entity'

export class ClientEntity {
    name?: string
    device: CreateDeviceEntity
    userId?: string
    clientId?: string
}
