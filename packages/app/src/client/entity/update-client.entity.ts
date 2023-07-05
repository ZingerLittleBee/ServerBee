import { UpdateDeviceEntity } from '@/client/device/entity/update-device.entity'

export class UpdateClientEntity {
    name?: string
    device?: UpdateDeviceEntity
    userId?: string
    clientId?: string
}
