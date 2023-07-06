import { Os, Overview, Process, Realtime } from '@server-octopus/types'

export class FusionDto {
    overview?: Overview

    os?: Os

    realtime?: Realtime

    process?: Process[]

    clientId?: string

    time?: number
}
