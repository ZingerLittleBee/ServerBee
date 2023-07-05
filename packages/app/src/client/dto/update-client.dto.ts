import { UpdateDeviceDto } from '@/client/device/dto/update-device.dto'
import { UpdateClientEntity } from '@/client/entity/update-client.entity'

export class UpdateClientDto {
    name?: string
    device?: UpdateDeviceDto
    userId?: string
    clientId?: string

    constructor(
        name?: string,
        device?: UpdateDeviceDto,
        userId?: string,
        clientId?: string
    ) {
        this.name = name
        this.device = device
        this.userId = userId
        this.clientId = clientId
    }

    toUpdateClientEntity(): UpdateClientEntity {
        return {
            name: this.name,
            device: this.device?.toUpdateDeviceEntity(),
            userId: this.userId,
            clientId: this.clientId
        }
    }
}
