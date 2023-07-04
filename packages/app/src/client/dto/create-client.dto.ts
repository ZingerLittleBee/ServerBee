import { CreateDeviceDto } from '@/client/device/dto/create-device.dto'

export class CreateClientDto {
    name?: string
    device: CreateDeviceDto
    userId?: string
    clientId?: string
}
