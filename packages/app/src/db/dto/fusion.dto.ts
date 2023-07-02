import { Os, Overview, Process, Realtime } from '@/db/schemas/fusion.type'

export class FusionDto {
    overview?: Overview

    os?: Os

    realtime?: Realtime

    process?: Process[]

    clientId?: string

    time?: number
}
