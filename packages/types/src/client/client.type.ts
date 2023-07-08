import { DiskDetailDto, NetworkInfoDto } from '../device.type'
import { Os, Overview, Process, Realtime, SimpleProcess } from '../fusion.type'


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


type FusionDto = {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    simpleProcess?: SimpleProcess[]
    fullProcess?: Process[]
    clientId?: string
    time?: number
}


export {
    ClientDto,
    DeviceDto,
    FusionDto
}
