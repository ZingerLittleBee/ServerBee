import { CreateDeviceEntity } from '@/client/device/entity/create-device.entity'

export class ClientEntity {
    name?: string
    device: CreateDeviceEntity
    userId?: string
    clientId?: string
}
