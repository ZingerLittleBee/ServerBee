import { DiskDetailEntity, NetworkInfoEntity } from '@server-octopus/types'

export class CreateDeviceEntity {
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
    disk: DiskDetailEntity[]
    network: NetworkInfoEntity[]
}
