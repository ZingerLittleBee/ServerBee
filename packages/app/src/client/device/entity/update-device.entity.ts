import { DiskDetail, NetworkInfo } from '@/types/device.type'

export class UpdateDeviceEntity {
    name?: string
    hostname?: string
    kernel?: string
    cpu_num?: number
    brand?: string
    frequency?: string
    vendor?: string
    memory?: string
    swap?: string
    version?: string
    disk?: DiskDetail[]
    network?: NetworkInfo[]
}
