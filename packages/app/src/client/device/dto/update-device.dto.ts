import { DiskDetail, NetworkInfo } from '@/types/device.type'
import { FormatData } from '@/types/fusion.type'
import { formatDataToString } from '@/utils/common.util'
import { UpdateDeviceEntity } from '@/client/device/entity/update-device.entity'

export class UpdateDeviceDto {
    name?: string
    hostname?: string
    kernel?: string
    cpu_num?: number
    brand?: string
    frequency?: string
    vendor?: string
    memory?: FormatData
    swap?: FormatData
    version?: string
    disk?: DiskDetail[]
    network?: NetworkInfo[]

    toUpdateDeviceEntity(): UpdateDeviceEntity {
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
