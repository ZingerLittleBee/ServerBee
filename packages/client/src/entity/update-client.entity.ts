import { UpdateDeviceEntity } from '@/device/entity/update-device.entity'

export class UpdateClientEntity {
    name?: string
    device?: UpdateDeviceEntity
    userId?: string
    clientId?: string
}
