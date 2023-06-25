export class CreateClientDto {
    name: string
    device: CreateDeviceDto
    userId: string
    clientId?: string
}
