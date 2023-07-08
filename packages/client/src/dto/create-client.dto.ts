import { CreateDeviceDto } from '@/device/dto/create-device.dto'

export class CreateClientDto {
    name?: string
    device: CreateDeviceDto
    userId?: string
    clientId?: string

    constructor(client: typeof CreateClientDto.prototype) {
        this.name = client.name
        this.device = new CreateDeviceDto(client.device)
        this.userId = client.userId
        this.clientId = client.clientId
    }
}
