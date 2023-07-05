import { DiskDetail, NetworkInfo } from '@/types/device.type'
import { FormatData } from '@/types/fusion.type'
import { CreateDeviceEntity } from '@/client/device/entity/create-device.entity'
import { formatDataToString } from '@/utils/common.util'

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

    toDeviceEntity(): CreateDeviceEntity {
        return {
            name: this.name,
            hostname: this.hostname,
            kernel: this.kernel,
            cpu_num: this.cpu_num,
            brand: this.brand,
            frequency: this.frequency,
            vendor: this.vendor,
            memory: formatDataToString(this.memory),
            swap: formatDataToString(this.swap),
            version: this.version,
            disk: this.disk,
            network: this.network
        }
    }
}
