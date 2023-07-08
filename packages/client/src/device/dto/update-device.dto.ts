import { DiskDetail, FormatData, NetworkInfo } from '@server-octopus/types'

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
}
