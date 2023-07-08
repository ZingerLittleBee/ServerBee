import { UpdateDeviceDto } from '@/device/dto/update-device.dto'

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
}
