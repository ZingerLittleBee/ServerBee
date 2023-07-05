import { FormatData } from '@app/shared/types/fusion.type'
import { DiskDetail, NetworkInfo } from '@app/shared/types/device.type'
import { UpdateDeviceEntity } from '@app/client/device/entity/update-device.entity'
import { formatDataToString } from '@app/shared/utils/common.util'

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
            disk: this.disk?.map((item) => ({
                ...item,
                total_space: formatDataToString(item.total_space),
                available_space: formatDataToString(item.available_space)
            })),
            network: this.network?.map((item) => ({
                ...item,
                rx: formatDataToString(item.rx),
                tx: formatDataToString(item.tx)
            }))
        }
    }
}
