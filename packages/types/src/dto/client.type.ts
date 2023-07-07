import { DiskDetailDto, NetworkInfoDto } from '../device.type'


type ClientDto = {
    name?: string
    device: DeviceDto
    userId?: string
    clientId?: string
}

type DeviceDto = {
    name: string
    hostname: string
    kernel: string
    cpu_num: number
    brand: string
    frequency: string
    vendor: string
    memory: string
    swap: string
    version: string
    disk: DiskDetailDto[]
    network: NetworkInfoDto[]
}

export {
    ClientDto,
    DeviceDto
}
