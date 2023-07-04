import { DiskDetail, NetworkInfo } from '@/types/device.type'
import { FormatData } from '@/types/fusion.type'

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
}
