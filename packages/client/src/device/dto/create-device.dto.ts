import { DiskDetail, FormatData, NetworkInfo } from '@server-octopus/types'
import { CreateDeviceEntity } from '@/device/entity/create-device.entity'
import { formatDataToString } from '@server-octopus/shared'

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
            disk: this.disk.map((item) => ({
                ...item,
                total_space: formatDataToString(item.total_space),
                available_space: formatDataToString(item.available_space)
            })),
            network: this.network.map((item) => ({
                ...item,
                rx: formatDataToString(item.rx),
                tx: formatDataToString(item.tx)
            }))
        }
    }
}
