import { Overview, Os, Realtime, Disk, Temp } from '@/db/schemas/fusion.schema'

export class CreateFusionDto {
    overview?: Overview

    os?: Os

    realtime?: Realtime

    disk?: Disk[]

    uptime?: number[]

    temp?: Temp[]

    clientId?: string

    time?: number
}
