import { DiskDetail, FormatData, NetworkInfo } from '@server-octopus/types'

export class CreateDeviceDto {
    name: string
    hostname: string
    kernel: string
    cpu_num: number
    brand: string
    frequency: string
    vendor: string
    memory: FormatData
    swap: FormatData
    version: string
    disk: DiskDetail[]
    network: NetworkInfo[]

    constructor(device: typeof CreateDeviceDto.prototype) {
        this.name = device.name
        this.hostname = device.hostname
        this.kernel = device.kernel
        this.cpu_num = device.cpu_num
        this.brand = device.brand
        this.frequency = device.frequency
        this.vendor = device.vendor
        this.memory = device.memory
        this.swap = device.swap
        this.version = device.version
        this.disk = device.disk
        this.network = device.network
    }
}
