import { UpdateDeviceEntity } from '@app/client/device/entity/update-device.entity'

export class UpdateClientEntity {
    name?: string
    device?: UpdateDeviceEntity
    userId?: string
    clientId?: string
}
