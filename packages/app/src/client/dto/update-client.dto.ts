import { CreateDeviceDto } from '@/client/device/dto/create-device.dto'

export class UpdateClientDto {
    name?: string
    device?: CreateDeviceDto
    userId?: string
    clientId?: string
}
